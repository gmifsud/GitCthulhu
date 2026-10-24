//! The graph, the inspector, the diff, and the files rail.
//!
//! Each preset is a different split tree over those panes. Ratios belong to
//! the preset that produced them: a width in Columns is a height when the
//! inspector sits over the diff.

use iced::widget::pane_grid::{Axis, Configuration, Node, Split, State};

/// Graph's share of the outer split. The wireframe's left column.
pub(crate) const GRAPH_RATIO: f32 = 0.34;
/// Inspector's share of the inspector/diff split when that split is a width.
pub(crate) const COLUMNS_MIDDLE_RATIO: f32 = 0.30;
/// Inspector's share of the inspector/diff split when that split is a height.
/// Both stacked presets use it.
pub(crate) const STACKED_MIDDLE_RATIO: f32 = 0.28;
/// Share of the post-graph split given to the inspector and the diff.
/// The files rail takes the rest.
pub(crate) const FILES_RATIO: f32 = 0.86;

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
    /// Graph | Inspector | Diff | Files
    Columns,
    /// Graph | (Inspector / Diff) | Files
    InspectorOverDiff,
    /// Graph / Inspector / Diff / Files
    Stacked,
}

impl LayoutPreset {
    pub(crate) const DEFAULT: Self = Self::InspectorOverDiff;

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Columns => 0,
            Self::InspectorOverDiff => 1,
            Self::Stacked => 2,
        }
    }
}

/// The three splitter positions for one preset.
///
/// `graph` is the outer split (graph against everything else). `middle` is the
/// inspector/diff split, vertical in [`LayoutPreset::Columns`] and horizontal
/// in the other two. `files` is the split whose far side is the files rail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayoutRatios {
    pub graph: f32,
    pub middle: f32,
    pub files: f32,
}

impl LayoutRatios {
    pub(crate) fn for_preset(preset: LayoutPreset) -> Self {
        Self {
            graph: GRAPH_RATIO,
            middle: match preset {
                LayoutPreset::Columns => COLUMNS_MIDDLE_RATIO,
                LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => STACKED_MIDDLE_RATIO,
            },
            files: FILES_RATIO,
        }
    }
}

pub(crate) fn default_ratios() -> [LayoutRatios; 3] {
    [
        LayoutRatios::for_preset(LayoutPreset::Columns),
        LayoutRatios::for_preset(LayoutPreset::InspectorOverDiff),
        LayoutRatios::for_preset(LayoutPreset::Stacked),
    ]
}

/// Which ratio a splitter writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RatioSlot {
    Graph,
    Middle,
    Files,
}

pub(crate) fn new_panes(preset: LayoutPreset, ratios: &LayoutRatios) -> State<WorkspacePane> {
    State::with_configuration(configuration(preset, ratios, true))
}

/// The same tree as [`new_panes`] without the files rail, used when the rail is collapsed.
pub(crate) fn new_panes_without_files(
    preset: LayoutPreset,
    ratios: &LayoutRatios,
) -> State<WorkspacePane> {
    State::with_configuration(configuration(preset, ratios, false))
}

/// Split handles produced by the latest [`new_panes`] or [`new_panes_without_files`].
/// The handles are new on every rebuild, so the caller replaces this map each time.
pub(crate) fn split_slots(state: &State<WorkspacePane>) -> Vec<(Split, RatioSlot)> {
    let mut slots = Vec::new();
    collect_slots(state.layout(), state, &mut slots);
    slots
}

fn configuration(
    preset: LayoutPreset,
    ratios: &LayoutRatios,
    files: bool,
) -> Configuration<WorkspacePane> {
    let middle = split(
        middle_axis(preset),
        ratios.middle,
        Configuration::Pane(WorkspacePane::Inspector),
        Configuration::Pane(WorkspacePane::Diff),
    );
    let rest = if files {
        split(
            outer_axis(preset),
            ratios.files,
            middle,
            Configuration::Pane(WorkspacePane::Files),
        )
    } else {
        middle
    };
    split(
        outer_axis(preset),
        ratios.graph,
        Configuration::Pane(WorkspacePane::Graph),
        rest,
    )
}

fn outer_axis(preset: LayoutPreset) -> Axis {
    match preset {
        LayoutPreset::Columns | LayoutPreset::InspectorOverDiff => Axis::Vertical,
        LayoutPreset::Stacked => Axis::Horizontal,
    }
}

fn middle_axis(preset: LayoutPreset) -> Axis {
    match preset {
        LayoutPreset::Columns => Axis::Vertical,
        LayoutPreset::InspectorOverDiff | LayoutPreset::Stacked => Axis::Horizontal,
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
    if left == [WorkspacePane::Graph] {
        Some(RatioSlot::Graph)
    } else if left == [WorkspacePane::Inspector] && right == [WorkspacePane::Diff] {
        Some(RatioSlot::Middle)
    } else if right == [WorkspacePane::Files] {
        Some(RatioSlot::Files)
    } else {
        None
    }
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
