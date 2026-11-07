//! The reported library-node aliases must preserve imported settings and connections.
use materialcraft_core::{Graph, Package, Value};
use materialcraft_formats::sbs::{read_sbs, write_sbs};

fn plain_xml(bytes: &[u8]) -> String {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    // Exercise XML import rather than the writer's embedded exact native-package copy.
    format!("{}</package>", text.split("<!--").next().unwrap())
}
const DEFS: [&str; 4] = ["histogram_scan", "auto_levels", "flood_fill", "height_blend"];

fn fixture(def: &str) -> String {
    let mut pkg = Package::new("Import check");
    let mut graph = Graph::new("G");
    let source = graph.add_node("uniform", [0.0, 0.0]);
    let second = (def == "height_blend").then(|| graph.add_node("uniform", [0.0, 150.0]));
    let filter = graph.add_node(def, [150.0, 0.0]);
    graph.connect(source, "output", filter, if def == "height_blend" { "top" } else { "input" }).unwrap();
    if let Some(second) = second {
        graph.connect(second, "output", filter, "bottom").unwrap();
    }
    if def == "histogram_scan" {
        let n = graph.node_mut(filter).unwrap();
        n.params.insert("position".into(), Value::Float(0.25));
        n.params.insert("contrast".into(), Value::Float(0.75));
    }
    pkg.graphs.push(graph);
    plain_xml(&write_sbs(&pkg).unwrap())
}
fn aliased_fixture(def: &str) -> String {
    let xml = fixture(def);
    match def {
        "histogram_scan" => xml
            .replace("<name v=\"position\"/>", "<name v=\"Position\"/>")
            .replace("<name v=\"contrast\"/>", "<name v=\"Contrast\"/>")
            .replace("<identifier v=\"input\"/>", "<identifier v=\"Input_1\"/>"),
        "auto_levels" => xml.replace("<identifier v=\"input\"/>", "<identifier v=\"Input\"/>"),
        "flood_fill" => xml.replace("<identifier v=\"input\"/>", "<identifier v=\"mask\"/>"),
        "height_blend" => {
            xml.replace("<identifier v=\"top\"/>", "<identifier v=\"height_top\"/>").replace("<identifier v=\"bottom\"/>", "<identifier v=\"height_bottom\"/>")
        }
        _ => panic!("unknown fixture node {def}"),
    }
}
fn assert_wires(pkg: &Package, def: &str) {
    let g = &pkg.graphs[0];
    let sources: Vec<_> = g.nodes.iter().filter(|n| n.def == "uniform").collect();
    let filter = g.nodes.iter().find(|n| n.def == def).unwrap();
    let ports: &[&str] = if def == "height_blend" { &["top", "bottom"] } else { &["input"] };
    assert_eq!(sources.len(), ports.len());
    assert_eq!(g.connections.len(), ports.len());
    for (source, port) in sources.iter().zip(ports) {
        let wire = g.input_of(filter.id, port).unwrap();
        assert_eq!((wire.from, wire.from_port.as_str(), wire.to, wire.to_port.as_str()), (source.id, "output", filter.id, *port));
    }
}

fn assert_scan_values(pkg: &Package) {
    let n = pkg.graphs[0].nodes.iter().find(|n| n.def == "histogram_scan").unwrap();
    assert_eq!(n.params.get("position"), Some(&Value::Float(0.25)));
    assert_eq!(n.params.get("contrast"), Some(&Value::Float(0.75)));
    assert!(!n.params.contains_key("Position") && !n.params.contains_key("Contrast"));
}

#[test]
fn canonical_names_keep_parameters_and_wires() {
    for def in DEFS {
        let read = read_sbs(fixture(def).as_bytes(), "check.sbs", &|_| None).unwrap();
        assert!(!read.exact);
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert_wires(&read.pkg, def);
        if def == "histogram_scan" {
            assert_scan_values(&read.pkg);
        }
    }
}
#[test]
fn histogram_scan_keeps_reported_parameter_spellings() {
    let xml = fixture("histogram_scan").replace("<name v=\"position\"/>", "<name v=\"Position\"/>").replace("<name v=\"contrast\"/>", "<name v=\"Contrast\"/>");
    let read = read_sbs(xml.as_bytes(), "check.sbs", &|_| None).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_scan_values(&read.pkg);
}
#[test]
fn histogram_scan_keeps_reported_input_connection() {
    let xml = fixture("histogram_scan").replace("<identifier v=\"input\"/>", "<identifier v=\"Input_1\"/>");
    let read = read_sbs(xml.as_bytes(), "check.sbs", &|_| None).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_wires(&read.pkg, "histogram_scan");
}
#[test]
fn auto_levels_keeps_reported_input_connection() {
    let xml = fixture("auto_levels").replace("<identifier v=\"input\"/>", "<identifier v=\"Input\"/>");
    let read = read_sbs(xml.as_bytes(), "check.sbs", &|_| None).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_wires(&read.pkg, "auto_levels");
}
#[test]
fn flood_fill_keeps_reported_mask_connection() {
    let read = read_sbs(aliased_fixture("flood_fill").as_bytes(), "check.sbs", &|_| None).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_wires(&read.pkg, "flood_fill");
}
#[test]
fn height_blend_keeps_reported_connections_without_swapping_sources() {
    let read = read_sbs(aliased_fixture("height_blend").as_bytes(), "check.sbs", &|_| None).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_wires(&read.pkg, "height_blend");
}
#[test]
fn aliased_imports_survive_a_structural_save_and_reload() {
    for def in DEFS {
        let read = read_sbs(aliased_fixture(def).as_bytes(), "check.sbs", &|_| None).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        let xml = plain_xml(&write_sbs(&read.pkg).unwrap());
        let back = read_sbs(xml.as_bytes(), "saved.sbs", &|_| None).unwrap();
        assert!(!back.exact);
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_wires(&back.pkg, def);
        if def == "histogram_scan" {
            assert_scan_values(&back.pkg);
        }
    }
}
#[test]
fn unknown_parameters_still_warn() {
    let xml = fixture("histogram_scan").replace("<name v=\"contrast\"/>", "<name v=\"UnexpectedContrast\"/>");
    let read = read_sbs(xml.as_bytes(), "check.sbs", &|_| None).unwrap();
    assert_eq!(read.warnings.len(), 1);
    assert!(read.warnings[0].contains("UnexpectedContrast"));
    let n = read.pkg.graphs[0].nodes.iter().find(|n| n.def == "histogram_scan").unwrap();
    assert_eq!(n.params.get("position"), Some(&Value::Float(0.25)));
    assert!(!n.params.contains_key("contrast") && !n.params.contains_key("UnexpectedContrast"));
}
#[test]
fn unknown_inputs_still_warn_and_drop_the_wire() {
    for def in DEFS {
        let original = if def == "height_blend" { "top" } else { "input" };
        let xml = fixture(def).replace(&format!("<identifier v=\"{original}\"/>"), "<identifier v=\"UnexpectedInput\"/>");
        let read = read_sbs(xml.as_bytes(), "check.sbs", &|_| None).unwrap();
        assert_eq!(read.warnings.len(), 1);
        assert!(read.warnings[0].contains("UnexpectedInput"));
        let g = &read.pkg.graphs[0];
        if def == "height_blend" {
            assert_eq!(g.connections.len(), 1);
            assert_eq!(g.connections[0].to_port, "bottom");
            assert_eq!(g.connections[0].from, g.nodes[1].id);
            assert_eq!(g.nodes.len(), 3);
        } else {
            assert!(g.connections.is_empty());
            assert_eq!(g.nodes.len(), 2);
        }
    }
}
