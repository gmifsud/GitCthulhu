//! Commit graph drawing: per-row geometry derived from `DagNode` lanes, painted on a canvas.
//!
//! Every commit row is the same height, so each row only draws its own slice of the
//! graph and the slices join up into continuous lines.

use std::cell::RefCell;

use core_domain::DagNode;
use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};

use crate::theme::hex_color;

/// Height of one commit row. Enough for a WIP dual-line description cell.
pub(crate) const ROW_HEIGHT: f32 = 40.0;
/// Horizontal distance between lane centres.
const LANE_SPACING: f32 = 16.0;
const DOT_RADIUS: f32 = 4.0;
const RING_RADIUS: f32 = 6.5;
const LINE_WIDTH: f32 = 2.0;
/// Lane colours, cycled by lane index.
const LANE_COLOURS: [u32; 6] = [0xF27D26, 0x3B82F6, 0x22C55E, 0xA855F7, 0xEC4899, 0x14B8A6];

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

fn lane_colour(lane: usize) -> Color {
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
}

/// Tessellated geometry kept between frames, with the row it was built from.
///
/// `GraphCell` is rebuilt on every `view`, so the cache lives in the widget state,
/// which iced keeps per position in the tree. It is thrown away when the row changes.
#[derive(Default)]
pub(crate) struct GraphCache {
    geometry: canvas::Cache,
    drawn: RefCell<Option<(GraphRow, NodeMark)>>,
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
        let key = (self.row.clone(), self.mark);
        if state.drawn.borrow().as_ref() != Some(&key) {
            state.geometry.clear();
            *state.drawn.borrow_mut() = Some(key);
        }
        let geometry = state.geometry.draw(renderer, bounds.size(), |frame| {
            paint_row(frame, &self.row, self.mark, bounds.width, bounds.height);
        });
        vec![geometry]
    }
}

fn paint_row(
    frame: &mut canvas::Frame,
    row: &GraphRow,
    mark: NodeMark,
    width: f32,
    bottom: f32,
) {
    let colour = lane_colour(row.lane);
    let dot = Point::new(lane_x(row.lane, width), bottom / 2.0);
    let mut segment = |from: Point, to: Point, lane: usize| {
        frame.stroke(
            &canvas::Path::line(from, to),
            canvas::Stroke::default()
                .with_color(lane_colour(lane))
                .with_width(LINE_WIDTH),
        );
    };

    for &lane in &row.passing {
        segment(
            Point::new(lane_x(lane, width), 0.0),
            Point::new(lane_x(lane, width), bottom),
            lane,
        );
    }
    for &lane in &row.incoming {
        segment(Point::new(lane_x(lane, width), 0.0), dot, lane);
    }
    for &lane in &row.outgoing {
        segment(dot, Point::new(lane_x(lane, width), bottom), lane);
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
