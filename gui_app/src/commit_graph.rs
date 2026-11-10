//! Commit graph drawing: per-row geometry derived from `DagNode` lanes, painted on a canvas.
//!
//! Every commit row is the same height, so each row only draws its own slice of the
//! graph and the slices join up into continuous lines.

use std::cell::RefCell;

use core_domain::DagNode;
use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};

use crate::theme::hex_color;

/// Height of one commit row: one line of text, pills inline.
pub(crate) const ROW_HEIGHT: f32 = 24.0;
/// Horizontal distance between lane centres.
const LANE_SPACING: f32 = 16.0;
const DOT_RADIUS: f32 = 4.0;
const RING_RADIUS: f32 = 6.5;
const LINE_WIDTH: f32 = 2.0;
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
}

/// Tessellated geometry kept between frames, with the row it was built from.
///
/// `GraphCell` is rebuilt on every `view`, so the cache lives in the widget state,
/// which iced keeps per position in the tree. It is thrown away when the row changes.
#[derive(Default)]
pub(crate) struct GraphCache {
    geometry: canvas::Cache,
    drawn: RefCell<Option<(GraphRow, NodeMark, RowInks)>>,
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
        let key = (self.row.clone(), self.mark, self.inks.clone());
        if state.drawn.borrow().as_ref() != Some(&key) {
            state.geometry.clear();
            *state.drawn.borrow_mut() = Some(key);
        }
        let geometry = state.geometry.draw(renderer, bounds.size(), |frame| {
            paint_row(frame, &self.row, self.mark, &self.inks, bounds.width, bounds.height);
        });
        vec![geometry]
    }
}

/// A segment's colour: its worktree's when one carries it, else its lane's.
fn ink_or_lane(inks: &[Option<Color>], index: usize, lane: usize) -> Color {
    inks.get(index).copied().flatten().unwrap_or_else(|| lane_colour(lane))
}

fn paint_row(
    frame: &mut canvas::Frame,
    row: &GraphRow,
    mark: NodeMark,
    inks: &RowInks,
    width: f32,
    bottom: f32,
) {
    let colour = inks.dot.unwrap_or_else(|| lane_colour(row.lane));
    let dot = Point::new(lane_x(row.lane, width), bottom / 2.0);
    let mut segment = |from: Point, to: Point, ink: Color| {
        frame.stroke(
            &canvas::Path::line(from, to),
            canvas::Stroke::default()
                .with_color(ink)
                .with_width(LINE_WIDTH),
        );
    };

    for (index, &lane) in row.passing.iter().enumerate() {
        segment(
            Point::new(lane_x(lane, width), 0.0),
            Point::new(lane_x(lane, width), bottom),
            ink_or_lane(&inks.passing, index, lane),
        );
    }
    for (index, &lane) in row.incoming.iter().enumerate() {
        segment(Point::new(lane_x(lane, width), 0.0), dot, ink_or_lane(&inks.incoming, index, lane));
    }
    for (index, &lane) in row.outgoing.iter().enumerate() {
        segment(dot, Point::new(lane_x(lane, width), bottom), ink_or_lane(&inks.outgoing, index, lane));
    }

    match mark {
        NodeMark::Remote => {
            frame.stroke(
                &canvas::Path::circle(dot, DOT_RADIUS + 1.0),
                canvas::Stroke::default()
                    .with_color(colour)
                    .with_width(LINE_WIDTH),
            );
        }
        NodeMark::Worktree => {
            frame.stroke(
                &canvas::Path::circle(dot, RING_RADIUS),
                canvas::Stroke::default()
                    .with_color(colour)
                    .with_width(1.5),
            );
            frame.fill(&canvas::Path::circle(dot, DOT_RADIUS), colour);
        }
        NodeMark::Local | NodeMark::Plain => {
            frame.fill(&canvas::Path::circle(dot, DOT_RADIUS), colour);
        }
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
