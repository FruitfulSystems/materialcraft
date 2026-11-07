#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreferenceTab {
    #[default]
    General,
    Graph,
    View3d,
}

impl PreferenceTab {
    pub const ALL: [Self; 3] = [Self::General, Self::Graph, Self::View3d];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Graph => "Graph",
            Self::View3d => "3D View",
        }
    }
}
