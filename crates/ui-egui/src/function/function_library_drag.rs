/// A library function being dragged onto a Function canvas
pub(crate) enum FunctionLibraryDrag {
    Operation(String),
    Instance { package: String, graph: String },
}
