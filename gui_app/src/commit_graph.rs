//! Commit graph drawing: per-row geometry derived from `DagNode` lanes, painted on a canvas.
//!
//! Every commit row is the same height, so each row only draws its own slice of the
//! graph and the slices join up into continuous lines.

use std::cell::RefCell;

use core_domain::DagNode;
use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};

use crate::theme::hex_color;

/// Height of every commit row: one line of text.
pub(crate) const ROW_HEIGHT: f32 = 26.0;
/// Horizontal distance between lane centres.
const LANE_SPACING: f32 = 16.0;
/// An ordinary commit: a small hollow ring.
const NODE_RADIUS: f32 = 3.5;
/// A branch tip: a filled ring a little larger.
const TIP_RADIUS: f32 = 4.5;
/// A worktree's tip: an outer ring round a filled one.
const RING_RADIUS: f32 = 6.0;
const LINE_WIDTH: f32 = 1.5;
/// How round a line's turn from one lane to another is.
const CORNER: f32 = 6.0;
/// The dotted line from a row's refs to its commit.
const LEADER_DASH: [f32; 2] = [2.0, 3.0];
/// Lane colours, cycled by lane index. Exported to the design system as `lane-0` … `lane-5`.
pub(crate) const LANE_COLOURS: [u32; 6] = [0xF27D26, 0x3B82F6, 0x22C55E, 0xA855F7, 0xEC4899, 0x14B8A6];

/// How the graph's lanes cross one commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphRow {
    /// The lane holding this row's commit dot.
    pub lane: usize,
    /// Lanes that end at this commit: drawn from the top of the row to the dot.
    pub incoming: Vec<usize>,
    /// Lanes heading to other commits: drawn straight through the row.
    pub passing: Vec<usize>,
    /// Lanes leaving towards this commit's parents: drawn from the dot to the bottom.
    pub outgoing: Vec<usize>,
}

/// Row geometry for `nodes`, in the same (newest first) order `build_dag` produced them.
pub(crate) fn graph_rows(nodes: &[DagNode]) -> Vec<GraphRow> {
    // `expected[i]` is the commit lane `i` is heading to below the current row.
    let mut expected: Vec<Option<&str>> = Vec::new();

    nodes
        .iter()
        .map(|node| {
            let mut incoming = Vec::new();
            let mut passing = Vec::new();
            for (lane, slot) in expected.iter_mut().enumerate() {
                match *slot {
                    Some(id) if id == node.commit_id => {
                        incoming.push(lane);
                        *slot = None;
                    }
                    Some(_) => passing.push(lane),
                    None => {}
                }
            }

            let mut outgoing = Vec::with_capacity(node.connections.len());
            for (parent, lane) in &node.connections {
                if expected.len() <= *lane {
                    expected.resize(*lane + 1, None);
                }
                expected[*lane] = Some(parent.as_str());
                outgoing.push(*lane);
            }
            while expected.last() == Some(&None) {
                expected.pop();
            }

            GraphRow {
                lane: node.lane,
                incoming,
                passing,
                outgoing,
            }
        })
        .collect()
}

/// Colours that replace a lane's own on one row, segment by segment, parallel to the lists in
/// its [`GraphRow`]. `None` keeps [`lane_colour`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RowInks {
    pub dot: Option<Color>,
    pub incoming: Vec<Option<Color>>,
    pub passing: Vec<Option<Color>>,
    pub outgoing: Vec<Option<Color>>,
}

/// Carry worktree colours down the graph. `tips[i]` is the colour of the worktree whose branch
/// tips at row `i`, if any. That row's dot takes it, and so does its lane from there down along
/// first parents, until the lane ends or another worktree's tip takes it over. Lanes no
/// worktree started keep their own colour.
pub(crate) fn row_inks(rows: &[GraphRow], tips: &[Option<Color>]) -> Vec<RowInks> {
    // `held[lane]`: the colour the line in that lane carries below the current row.
    let mut held: Vec<Option<Color>> = Vec::new();
    let read = |held: &[Option<Color>], lane: usize| held.get(lane).copied().flatten();
    let write = |held: &mut Vec<Option<Color>>, lane: usize, ink: Option<Color>| {
        if held.len() <= lane {
            held.resize(lane + 1, None);
        }
        held[lane] = ink;
    };
    rows.iter()
        .zip(tips.iter().chain(std::iter::repeat(&None)))
        .map(|(row, tip)| {
            let incoming: Vec<Option<Color>> = row.incoming.iter().map(|&lane| read(&held, lane)).collect();
            let passing: Vec<Option<Color>> = row.passing.iter().map(|&lane| read(&held, lane)).collect();
            let inherited = row.incoming.contains(&row.lane).then(|| read(&held, row.lane)).flatten();
            let dot = tip.or(inherited);
            // Lanes ending at this commit drop what they carried; a lane claimed afresh below
            // starts plain.
            for &lane in &row.incoming {
                write(&mut held, lane, None);
            }
            let outgoing: Vec<Option<Color>> = row
                .outgoing
                .iter()
                .map(|&lane| if lane == row.lane { dot } else { read(&held, lane) })
                .collect();
            if row.outgoing.contains(&row.lane) {
                write(&mut held, row.lane, dot);
            }
            RowInks { dot, incoming, passing, outgoing }
        })
        .collect()
}

/// Width needed to draw every lane used by `rows`.
pub(crate) fn graph_width(rows: &[GraphRow]) -> f32 {
    let lanes = rows
        .iter()
        .flat_map(|row| {
            row.incoming
                .iter()
                .chain(&row.passing)
                .chain(&row.outgoing)
                .chain([&row.lane])
        })
        .max()
        .map_or(0, |widest| widest + 1);
    lanes as f32 * LANE_SPACING
}

/// Lane centres grow right from the left edge so the tip (lane 0) sits
/// against the left gutter — pills can touch the node.
fn lane_x(lane: usize, _width: f32) -> f32 {
    LANE_SPACING * (lane as f32 + 0.5)
}

pub(crate) fn lane_colour(lane: usize) -> Color {
    hex_color(LANE_COLOURS[lane % LANE_COLOURS.len()])
}

/// How the commit tip is marked on the canvas (local / remote / worktree).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NodeMark {
    /// Ordinary commit with no tip refs, or unknown.
    #[default]
    Plain,
    /// Local branch tip (including HEAD).
    Local,
    /// Remote-tracking tip only.
    Remote,
    /// Local branch checked out in a worktree.
    Worktree,
}

/// Canvas program that paints one row's slice of the graph.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GraphCell {
    pub row: GraphRow,
    pub mark: NodeMark,
    pub inks: RowInks,
    /// The row has refs: a dotted line runs in from the left edge to the commit, carrying on
    /// the one drawn after the refs in their column.
    pub leader: Option<Color>,
}

/// Tessellated geometry kept between frames, with the row it was built from.
///
/// `GraphCell` is rebuilt on every `view`, so the cache lives in the widget state,
/// which iced keeps per position in the tree. It is thrown away when the row changes.
#[derive(Default)]
pub(crate) struct GraphCache {
    geometry: canvas::Cache,
    drawn: RefCell<Option<(GraphRow, NodeMark, RowInks, Option<Color>)>>,
}

impl<Message> canvas::Program<Message> for GraphCell {
    type State = GraphCache;

    fn draw(
        &self,
        state: &GraphCache,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let key = (self.row.clone(), self.mark, self.inks.clone(), self.leader);
        if state.drawn.borrow().as_ref() != Some(&key) {
            state.geometry.clear();
            *state.drawn.borrow_mut() = Some(key);
        }
        let geometry = state.geometry.draw(renderer, bounds.size(), |frame| {
            paint_row(frame, &self.row, self.mark, &self.inks, self.leader, bounds.width, bounds.height);
        });
        vec![geometry]
    }
}

/// A segment's colour: its worktree's when one carries it, else its lane's.
fn ink_or_lane(inks: &[Option<Color>], index: usize, lane: usize) -> Color {
    inks.get(index).copied().flatten().unwrap_or_else(|| lane_colour(lane))
}

/// How far a line stops short of a node's centre, so it meets the ring's edge, never its inside.
fn node_radius(mark: NodeMark) -> f32 {
    match mark {
        NodeMark::Plain => NODE_RADIUS,
        NodeMark::Local | NodeMark::Remote => TIP_RADIUS,
        NodeMark::Worktree => RING_RADIUS,
    }
}

/// A line from `from` to `to` that runs straight along `from`'s lane, then turns with a
/// rounded corner and runs level into `to`: how a branch comes in to the commit it forks from.
fn turn_in(from: Point, to: Point) -> canvas::Path {
    let side = (to.x - from.x).signum();
    canvas::Path::new(|path| {
        path.move_to(from);
        path.line_to(Point::new(from.x, to.y - CORNER));
        path.quadratic_curve_to(Point::new(from.x, to.y), Point::new(from.x + side * CORNER, to.y));
        path.line_to(to);
    })
}

/// The mirror of [`turn_in`]: level out of `from`, a rounded turn, then down `to`'s lane: how
/// a merge's second parent leaves it.
fn turn_out(from: Point, to: Point) -> canvas::Path {
    let side = (to.x - from.x).signum();
    canvas::Path::new(|path| {
        path.move_to(from);
        path.line_to(Point::new(to.x - side * CORNER, from.y));
        path.quadratic_curve_to(Point::new(to.x, from.y), Point::new(to.x, from.y + CORNER));
        path.line_to(to);
    })
}

fn paint_row(
    frame: &mut canvas::Frame,
    row: &GraphRow,
    mark: NodeMark,
    inks: &RowInks,
    leader: Option<Color>,
    width: f32,
    bottom: f32,
) {
    let colour = inks.dot.unwrap_or_else(|| lane_colour(row.lane));
    let dot = Point::new(lane_x(row.lane, width), bottom / 2.0);
    let gap = node_radius(mark) + 1.0;
    let line = |ink: Color| canvas::Stroke::default().with_color(ink).with_width(LINE_WIDTH);

    for (index, &lane) in row.passing.iter().enumerate() {
        let x = lane_x(lane, width);
        frame.stroke(&canvas::Path::line(Point::new(x, 0.0), Point::new(x, bottom)), line(ink_or_lane(&inks.passing, index, lane)));
    }
    for (index, &lane) in row.incoming.iter().enumerate() {
        let x = lane_x(lane, width);
        let ink = line(ink_or_lane(&inks.incoming, index, lane));
        if lane == row.lane {
            frame.stroke(&canvas::Path::line(Point::new(x, 0.0), Point::new(x, dot.y - gap)), ink);
        } else {
            let side = (dot.x - x).signum();
            frame.stroke(&turn_in(Point::new(x, 0.0), Point::new(dot.x - side * gap, dot.y)), ink);
        }
    }
    for (index, &lane) in row.outgoing.iter().enumerate() {
        let x = lane_x(lane, width);
        let ink = line(ink_or_lane(&inks.outgoing, index, lane));
        if lane == row.lane {
            frame.stroke(&canvas::Path::line(Point::new(x, dot.y + gap), Point::new(x, bottom)), ink);
        } else {
            let side = (x - dot.x).signum();
            frame.stroke(&turn_out(Point::new(dot.x + side * gap, dot.y), Point::new(x, bottom)), ink);
        }
    }
    if let Some(ink) = leader {
        let end = dot.x - gap - 1.0;
        if end > 0.0 {
            frame.stroke(
                &canvas::Path::line(Point::new(0.0, dot.y), Point::new(end, dot.y)),
                canvas::Stroke {
                    line_dash: canvas::LineDash { segments: &LEADER_DASH, offset: 0 },
                    ..canvas::Stroke::default().with_color(ink).with_width(1.0)
                },
            );
        }
    }

    let ring = |radius: f32| canvas::Path::circle(dot, radius);
    match mark {
        NodeMark::Plain => frame.stroke(&ring(NODE_RADIUS), line(colour)),
        NodeMark::Remote => frame.stroke(&ring(TIP_RADIUS), line(colour)),
        NodeMark::Local => frame.fill(&ring(TIP_RADIUS), colour),
        NodeMark::Worktree => {
            frame.stroke(&ring(RING_RADIUS), line(colour));
            frame.fill(&ring(NODE_RADIUS), colour);
        }
    }
}

/// The dotted line after a row's refs, out to the edge of their column; the graph carries it on
/// to the commit. Drawn across whatever width it is given, at mid-height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LeaderLine {
    pub colour: Color,
}

impl<Message> canvas::Program<Message> for LeaderLine {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let y = bounds.height / 2.0;
        frame.stroke(
            &canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y)),
            canvas::Stroke {
                line_dash: canvas::LineDash { segments: &LEADER_DASH, offset: 0 },
                ..canvas::Stroke::default().with_color(self.colour).with_width(1.0)
            },
        );
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::{build_dag, Commit};

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: parents.iter().map(|p| p.to_string()).collect(),
            author: "Test Author".to_string(),
            message: format!("commit {id}"),
            timestamp: 0,
        }
    }

    fn row(lane: usize, incoming: &[usize], passing: &[usize], outgoing: &[usize]) -> GraphRow {
        GraphRow {
            lane,
            incoming: incoming.to_vec(),
            passing: passing.to_vec(),
            outgoing: outgoing.to_vec(),
        }
    }

    #[test]
    fn linear_history_is_one_continuous_lane() {
        let nodes = build_dag(&[
            commit("c3", &["c2"]),
            commit("c2", &["c1"]),
            commit("c1", &[]),
        ]);

        assert_eq!(
            graph_rows(&nodes),
            vec![
                row(0, &[], &[], &[0]),
                row(0, &[0], &[], &[0]),
                row(0, &[0], &[], &[])
            ]
        );
    }

    #[test]
    fn merge_branches_out_and_converges() {
        let nodes = build_dag(&[
            commit("M", &["A", "B"]),
            commit("A", &["base"]),
            commit("B", &["base"]),
            commit("base", &[]),
        ]);

        assert_eq!(
            graph_rows(&nodes),
            vec![
                row(0, &[], &[], &[0, 1]), // M opens lane 1 for B
                row(0, &[0], &[1], &[0]),  // A: lane 1 passes by
                row(1, &[1], &[0], &[1]),  // B: lane 0 passes by
                row(0, &[0, 1], &[], &[]), // base: both lanes end here
            ]
        );
    }

    #[test]
    fn a_worktree_colour_runs_down_its_lane_and_stops_where_the_lane_ends() {
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        // T1 (lane 0) and T2 (lane 1) both lead to base; T2 is a worktree's tip.
        let nodes = build_dag(&[commit("T1", &["base"]), commit("T2", &["base"]), commit("base", &[])]);
        let rows = graph_rows(&nodes);
        let inks = row_inks(&rows, &[None, Some(blue), None]);

        assert_eq!(inks[0], RowInks { dot: None, incoming: vec![], passing: vec![], outgoing: vec![None] });
        // T2: its dot and the line below it are the worktree's; lane 0 passes by in its own colour.
        assert_eq!(inks[1], RowInks { dot: Some(blue), incoming: vec![], passing: vec![None], outgoing: vec![Some(blue)] });
        // base: lane 1 arrives blue and ends; base itself sits in lane 0, which no worktree started.
        assert_eq!(inks[2].incoming, vec![None, Some(blue)]);
        assert_eq!(inks[2].dot, None);
    }

    #[test]
    fn a_worktree_lower_down_a_lane_takes_it_over_from_its_tip() {
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let green = Color::from_rgb(0.0, 1.0, 0.0);
        let nodes = build_dag(&[commit("c3", &["c2"]), commit("c2", &["c1"]), commit("c1", &[])]);
        let inks = row_inks(&graph_rows(&nodes), &[Some(blue), Some(green), None]);
        assert_eq!(inks[0].outgoing, vec![Some(blue)]);
        assert_eq!(inks[1].incoming, vec![Some(blue)], "the line above the second tip is still the first's");
        assert_eq!((inks[1].dot, inks[1].outgoing.clone()), (Some(green), vec![Some(green)]));
        assert_eq!(inks[2].dot, Some(green));
    }

    #[test]
    fn a_merge_keeps_the_worktree_colour_on_its_first_parent_only() {
        let blue = Color::from_rgb(0.0, 0.0, 1.0);
        let nodes = build_dag(&[
            commit("M", &["A", "B"]),
            commit("A", &["base"]),
            commit("B", &["base"]),
            commit("base", &[]),
        ]);
        let inks = row_inks(&graph_rows(&nodes), &[Some(blue)]);
        assert_eq!(inks[0].outgoing, vec![Some(blue), None], "the merged-in side keeps its own colour");
        assert_eq!(inks[1].dot, Some(blue));
        assert_eq!(inks[2].dot, None);
    }

    #[test]
    fn branch_tip_starts_without_an_incoming_line() {
        let nodes = build_dag(&[
            commit("T1", &["base"]),
            commit("T2", &["base"]),
            commit("base", &[]),
        ]);

        let rows = graph_rows(&nodes);

        assert_eq!(rows[1], row(1, &[], &[0], &[1]));
    }

    #[test]
    fn width_covers_the_widest_lane() {
        let nodes = build_dag(&[
            commit("M", &["A", "B", "C"]),
            commit("A", &[]),
            commit("B", &[]),
            commit("C", &[]),
        ]);

        assert_eq!(graph_width(&graph_rows(&nodes)), 3.0 * LANE_SPACING);
        assert_eq!(graph_width(&[]), 0.0);
    }
}
