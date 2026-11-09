use super::*;
use materialcraft_core::Graph;

#[test]
fn frame_titles_map_to_library_categories() {
    assert_eq!(frame_category("Texture Generators - Noises", false, false).as_deref(), Some("Texture generators/Noises"));
    assert_eq!(frame_category("Spline & Path tools - Path tools", false, false).as_deref(), Some("Spline & Path tools/Path tools"));
    assert_eq!(frame_category("Material Filters - 1 click", false, false).as_deref(), Some("Material filters/1-click"));
    assert_eq!(frame_category("Mesh-based generators - Weathering", false, false).as_deref(), Some("Mesh-based generators/Weathering"));
    assert_eq!(frame_category("PBR Materials", false, false).as_deref(), Some("PBR materials"));
    assert_eq!(frame_category("Atomic Nodes", false, false).as_deref(), Some("Atomic nodes"));
    // The category MaterialCraft renamed still reads from older layouts
    assert_eq!(frame_category("SDF functions - Primitives", false, false).as_deref(), Some("MCF functions/Primitives"));
    assert_eq!(frame_category("Functions Inside", false, false), None);
    assert_eq!(frame_category("Filters - Nowhere", false, false), None);
    assert_eq!(frame_category("Comparison", true, true).as_deref(), Some("Function nodes/Comparison"));
    assert_eq!(frame_category("Comparison", true, false).as_deref(), Some("Functions/Comparison"));
    assert_eq!(frame_category("Unknown", true, true), None);
}

#[test]
fn identifiers_read_as_labels() {
    assert_eq!(humanize("3D_perlin_noise"), "3D Perlin Noise");
    assert_eq!(humanize("bnw_spots_1"), "BnW Spots 1");
    assert_eq!(humanize("rgb-a_merge"), "RGB-A Merge");
    assert_eq!(humanize("Functions/Math/acescg_to_srgb"), "ACEScg To sRGB");
    assert_eq!(humanize("__"), "");
}

#[test]
fn the_bundled_library_fills_every_category_of_the_tree() {
    let library = NodeLibrary::shared();
    assert_eq!(library.layouts, vec!["sbs://all_nodes.sbs".to_string()]);
    assert!(library.packages.contains_key("sbs://functions.sbs"));
    assert!(!library.packages.contains_key("sbs://all_nodes.sbs"), "a layout is not a package of graphs");

    let entries = library.entries(&library.packages);
    let ids: BTreeSet<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids.len(), entries.len(), "ids are unique");
    let known = |category: &str| {
        let mut parts = category.split('/');
        let top = parts.next().unwrap_or("");
        CATEGORIES.iter().any(|(name, subs)| *name == top && parts.next().is_none_or(|sub| subs.contains(&sub)))
    };
    for entry in &entries {
        assert!(known(&entry.category), "{} sits in unknown category {}", entry.id, entry.category);
    }
    let count = |category: &str| entries.iter().filter(|entry| entry.category == category).count();
    let find = |id: &str| entries.iter().find(|entry| entry.id == id).unwrap_or_else(|| panic!("no entry {id}"));

    // Every top-level category that has sub-categories, except the new ones, has entries
    for (top, subs) in CATEGORIES {
        for sub in *subs {
            let category = format!("{top}/{sub}");
            if !matches!(*top, "MCF functions" | "Values") && !matches!(category.as_str(), "PBR materials/Pavement" | "PBR materials/Stone") {
                assert!(count(&category) > 0, "{category} is empty");
            }
        }
    }
    assert_eq!(count("Texture generators/Noises"), 89);
    assert_eq!(
        entries.iter().filter(|entry| entry.category == "Graph items").map(|entry| entry.label.as_str()).collect::<Vec<_>>(),
        ["Comment", "Frame", "Pin", "Dot node"]
    );
    assert_eq!(find("node:blur").category, "Atomic nodes");
    assert_eq!(find("node:output").category, "Atomic nodes");
    assert!(find("node:bitmap").missing.is_none(), "the atomic frame's bitmap is the Bitmap node");
    assert!(entries.iter().filter(|entry| entry.category == "Atomic nodes").all(|entry| entry.missing.is_none()));
    assert!(entries.iter().filter(|entry| entry.category == "3D view/HDRI environments").all(|entry| entry.missing.is_some()));
    assert!(find("node:blur").missing.is_none());
    assert_eq!(find("function:add").category, "Function nodes/Operator");
    assert_eq!(find("pattern:paramset").category, "FxMap nodes");

    let pi = find("sbs://functions.sbs#Functions/Math/Pi");
    assert_eq!((pi.label.as_str(), pi.category.as_str()), ("Pi", "Functions/Constant"));
    assert!(pi.missing.is_none());
    assert_eq!(find("sbs://functions.sbs#Functions/Math/Equality_Float2").category, "Functions/Comparison");

    let perlin = find("sbs://3d_perlin_noise.sbs#3D_perlin_noise");
    assert_eq!((perlin.label.as_str(), perlin.category.as_str()), ("3D Perlin Noise", "Texture generators/Noises"));
    assert!(perlin.missing.is_some(), "library packages MaterialCraft lacks are listed but not addable");
    assert_eq!(find("sbs://materials/pbr/bricks_001.sbs#bricks_001").category, "PBR materials/Bricks");
    assert_eq!(find("sbs://materials/pbr/old_painted_planks.sbs#old_painted_planks").category, "PBR materials/Wood");
    // A node that sits in two frames is listed in both
    assert!(entries.iter().filter(|entry| entry.id.starts_with("sbs://paths_tools.sbs#path_to_spline")).count() == 2);
}

#[test]
fn a_library_package_makes_its_placed_entry_available() {
    let library = NodeLibrary::shared();
    let mut dependencies = library.packages.clone();
    let mut graph = Graph::new("3D_perlin_noise");
    graph.add_node("uniform", [0.0; 2]);
    graph.add_node("output", [200.0, 0.0]);
    let mut package = Package::new("3d_perlin_noise");
    package.graphs.push(graph.clone());
    graph.name = "Loose".into();
    graph.attributes.category = "Filters/Blurs".into();
    package.graphs.push(graph);
    dependencies.insert("sbs://3d_perlin_noise.sbs".into(), Arc::new(package));

    let entries = library.entries(&dependencies);
    let perlin = entries.iter().find(|entry| entry.id == "sbs://3d_perlin_noise.sbs#3D_perlin_noise").unwrap();
    assert!(perlin.missing.is_none());
    assert_eq!(perlin.category, "Texture generators/Noises");
    let loose = entries.iter().find(|entry| entry.id == "sbs://3d_perlin_noise.sbs#Loose").unwrap();
    assert_eq!((loose.category.as_str(), loose.canvas), ("Filters/Blurs", LibraryCanvas::Material));
}

#[test]
fn unreadable_library_files_warn_instead_of_failing() {
    let library = NodeLibrary::load(&["sbs://does_not_exist.sbs".into(), "sbs://../escape.sbs".into()]);
    assert!(library.packages.is_empty() && library.layouts.is_empty());
    assert_eq!(library.warnings.len(), 2);
    let entries = library.entries(&BTreeMap::new());
    assert!(entries.iter().any(|entry| entry.id == "node:blur" && entry.category == "Atomic nodes"));
    assert!(entries.iter().any(|entry| entry.id == "function:add" && entry.category == "Function nodes/Operator"));
}
