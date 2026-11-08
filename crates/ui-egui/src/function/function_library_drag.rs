pub(super) enum FunctionLibraryDrag {
    Operation(String),
    Instance { package: String, graph: String },
}
