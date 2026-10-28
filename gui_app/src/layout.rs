//! Staging on the left, history in the middle, inspector on the right.
//!
//! Default preset stacks the graph above the diff in the middle column so the
//! description column keeps width while Diff still has a full middle span.

use iced::widget::pane_grid::{Axis, Configuration, Node, Split, State};

/// Left staging column share of the window.
pub(crate) const FILES_RATIO: f32 = 0.22;
/// Of the remainder, share given to Graph (+ Diff) versus the Inspector.
pub(crate) const GRAPH_RATIO: f32 = 0.68;
/// Graph's share of the Graph/Diff split when that split is a height. History is what you
/// read first; the diff below it is often empty until something is selected.
pub(crate) const STACKED_MIDDLE_RATIO: f32 = 0.62;
/// Top (History | Inspector) share of the window height in the Log preset; the diff spans
/// the full width underneath, as in the JetBrains Git log.
pub(crate) const LOG_MIDDLE_RATIO: f32 = 0.64;
/// Diff's share of Diff|Inspector when Columns places them side by side.
pub(crate) const COLUMNS_MIDDLE_RATIO: f32 = 0.50;

/// Floor for the left staging ratio (avoids crushing Stage).
pub(crate) const MIN_FILES_RATIO: f32 = 0.16;
pub(crate) const MAX_FILES_RATIO: f32 = 0.36;
/// Floor for the middle history share of the post-staging remainder.
pub(crate) const MIN_GRAPH_RATIO: f32 = 0.50;
pub(crate) const MAX_GRAPH_RATIO: f32 = 0.82;
/// Centre column share (Graph+Diff vs Inspector) when [`HunkMode::Wide`] is active.
pub(crate) const WIDE_GRAPH_RATIO: f32 = 0.78;
/// Graph share of the Graph/Diff stack when Wide is active (more room for Diff).
pub(crate) const WIDE_MIDDLE_RATIO: f32 = 0.36;

/// Diff presentation relative to the graph and side rails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HunkMode {
    #[default]
    Beside,
    Wide,
    Focus,
}

impl HunkMode {
    /// Toolbar cycle: Beside ↔ Wide (Focus is explicit only).
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::Beside => Self::Wide,
            Self::Wide | Self::Focus => Self::Beside,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspacePane {
    Graph,
    Inspector,
    Diff,
    Files,
}

/// Which arrangement the top bar is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutPreset {
    /// JetBrains Git log: Files | (Graph | Inspector) over a full-width Diff.
    Log,
    /// Files | Graph | Diff | Inspector
    Columns,
    /// Files | (Graph / Diff) | Inspector
    InspectorOverDiff,
    /// Files / Graph / Diff / Inspector
    Stacked,
}

impl LayoutPreset {
    pub(crate) const DEFAULT: Self = Self::Log;
    pub(crate) const COUNT: usize = 4;

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Columns => 0,
            Self::InspectorOverDiff => 1,
            Self::Stacked => 2,
            Self::Log => 3,
        }
    }

    /// Next preset in the top-bar cycle (Ctrl+Shift+L).
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Log => Self::Columns,
            Self::Columns => Self::InspectorOverDiff,
            Self::InspectorOverDiff => Self::Stacked,
            Self::Stacked => Self::Log,
        }
    }
}

/// The three splitter positions for one preset.
///
/// `files` is the outer split (staging against everything else). `graph` is the
/// split between the history column (or history+diff stack) and the inspector.
/// `middle` is Graph vs Diff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayoutRatios {
    pub graph: f32,
    pub middle: f32,
    pub files: f32,
}

impl LayoutRatios {
    pub(crate) fn for_preset(preset: LayoutPreset) -> Self {
        Self {
            files: FILES_RATIO,
            graph: GRAPH_RATIO,
            middle: match preset {
                LayoutPreset::Log => LOG_MIDDLE_RATIO,
                LayoutPreset::Columns => COLUMNS_MIDDLE_RATIO,
                LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => STACKED_MIDDLE_RATIO,
            },
        }
    }

    pub(crate) fn for_hunk_mode(self, mode: HunkMode) -> Self {
        match mode {
            HunkMode::Beside | HunkMode::Focus => self,
            HunkMode::Wide => Self {
                graph: self.graph.clamp(WIDE_GRAPH_RATIO, MAX_GRAPH_RATIO),
                middle: self.middle.min(WIDE_MIDDLE_RATIO),
                files: self.files,
            },
        }
    }

    pub(crate) fn clamp_slot(&mut self, slot: RatioSlot, ratio: f32) {
        match slot {
            RatioSlot::Files => {
                self.files = ratio.clamp(MIN_FILES_RATIO, MAX_FILES_RATIO);
            }
            RatioSlot::Graph => {
                self.graph = ratio.clamp(MIN_GRAPH_RATIO, MAX_GRAPH_RATIO);
            }
            RatioSlot::Middle => {
                self.middle = ratio.clamp(0.25, 0.75);
            }
        }
    }
}

pub(crate) fn default_ratios() -> [LayoutRatios; LayoutPreset::COUNT] {
    [
        LayoutRatios::for_preset(LayoutPreset::Columns),
        LayoutRatios::for_preset(LayoutPreset::InspectorOverDiff),
        LayoutRatios::for_preset(LayoutPreset::Stacked),
        LayoutRatios::for_preset(LayoutPreset::Log),
    ]
}

/// Which ratio a splitter writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RatioSlot {
    Graph,
    Middle,
    Files,
}

pub(crate) fn new_panes(
    preset: LayoutPreset,
    ratios: &LayoutRatios,
    show_files: bool,
    show_inspector: bool,
) -> State<WorkspacePane> {
    State::with_configuration(configuration(
        preset,
        ratios,
        show_files,
        show_inspector,
    ))
}

/// Split handles produced by the latest [`new_panes`] call.
pub(crate) fn split_slots(state: &State<WorkspacePane>) -> Vec<(Split, RatioSlot)> {
    let mut slots = Vec::new();
    collect_slots(state.layout(), state, &mut slots);
    slots
}

fn configuration(
    preset: LayoutPreset,
    ratios: &LayoutRatios,
    show_files: bool,
    show_inspector: bool,
) -> Configuration<WorkspacePane> {
    if preset == LayoutPreset::Log {
        let top = if show_inspector {
            split(
                Axis::Vertical,
                ratios.graph,
                Configuration::Pane(WorkspacePane::Graph),
                Configuration::Pane(WorkspacePane::Inspector),
            )
        } else {
            Configuration::Pane(WorkspacePane::Graph)
        };
        let centre = split(Axis::Horizontal, ratios.middle, top, Configuration::Pane(WorkspacePane::Diff));
        return if show_files {
            split(Axis::Vertical, ratios.files, Configuration::Pane(WorkspacePane::Files), centre)
        } else {
            centre
        };
    }
    let history = match preset {
        LayoutPreset::Columns => {
            if show_inspector {
                split(
                    Axis::Vertical,
                    ratios.middle,
                    Configuration::Pane(WorkspacePane::Diff),
                    Configuration::Pane(WorkspacePane::Inspector),
                )
            } else {
                Configuration::Pane(WorkspacePane::Diff)
            }
        }
        LayoutPreset::Log | LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => split(
            middle_axis(preset),
            ratios.middle,
            Configuration::Pane(WorkspacePane::Graph),
            Configuration::Pane(WorkspacePane::Diff),
        ),
    };

    let centre = match preset {
        LayoutPreset::Columns => split(
            Axis::Vertical,
            ratios.graph,
            Configuration::Pane(WorkspacePane::Graph),
            history,
        ),
        LayoutPreset::Log | LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => {
            if show_inspector {
                split(
                    outer_axis(preset),
                    ratios.graph,
                    history,
                    Configuration::Pane(WorkspacePane::Inspector),
                )
            } else {
                history
            }
        }
    };

    if show_files {
        split(
            outer_axis(preset),
            ratios.files,
            Configuration::Pane(WorkspacePane::Files),
            centre,
        )
    } else {
        centre
    }
}

fn outer_axis(preset: LayoutPreset) -> Axis {
    match preset {
        LayoutPreset::Log | LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => Axis::Vertical,
        LayoutPreset::Stacked => Axis::Horizontal,
    }
}

fn middle_axis(preset: LayoutPreset) -> Axis {
    match preset {
        LayoutPreset::Columns => Axis::Vertical,
        LayoutPreset::Log | LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => Axis::Horizontal,
    }
}

fn split(
    axis: Axis,
    ratio: f32,
    a: Configuration<WorkspacePane>,
    b: Configuration<WorkspacePane>,
) -> Configuration<WorkspacePane> {
    Configuration::Split {
        axis,
        ratio,
        a: Box::new(a),
        b: Box::new(b),
    }
}

fn collect_slots(node: &Node, state: &State<WorkspacePane>, slots: &mut Vec<(Split, RatioSlot)>) {
    let Node::Split { id, a, b, .. } = node else {
        return;
    };
    if let Some(slot) = slot_for(&panes_under(a, state), &panes_under(b, state)) {
        slots.push((*id, slot));
    }
    collect_slots(a, state, slots);
    collect_slots(b, state, slots);
}

fn slot_for(left: &[WorkspacePane], right: &[WorkspacePane]) -> Option<RatioSlot> {
    if left == [WorkspacePane::Files] {
        return Some(RatioSlot::Files);
    }
    if left == [WorkspacePane::Graph] && right == [WorkspacePane::Diff] {
        return Some(RatioSlot::Middle);
    }
    // Log preset: the (History | Inspector) row against the full-width Diff.
    if left == [WorkspacePane::Graph, WorkspacePane::Inspector] && right == [WorkspacePane::Diff] {
        return Some(RatioSlot::Middle);
    }
    if left == [WorkspacePane::Diff] && right == [WorkspacePane::Inspector] {
        return Some(RatioSlot::Middle);
    }
    if left == [WorkspacePane::Graph] {
        return Some(RatioSlot::Graph);
    }
    if left == [WorkspacePane::Graph, WorkspacePane::Diff]
        || (left.contains(&WorkspacePane::Graph) && left.contains(&WorkspacePane::Diff))
    {
        return Some(RatioSlot::Graph);
    }
    None
}

fn panes_under(node: &Node, state: &State<WorkspacePane>) -> Vec<WorkspacePane> {
    match node {
        Node::Split { a, b, .. } => {
            let mut panes = panes_under(a, state);
            panes.extend(panes_under(b, state));
            panes
        }
        Node::Pane(pane) => state.get(*pane).copied().into_iter().collect(),
    }
}
