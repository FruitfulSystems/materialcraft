//! SBS packages, MaterialCraft's main project format, read and written with clean-room code built from public documentation and samples.
//! Reading keeps going past unknown nodes: they become [`MISSING_NODE`] placeholders and each gap is in [`SbsRead::warnings`].
//! Writing embeds bitmaps and adds an exact copy of the package in a trailing comment, used while the `<content>` hash matches.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::io::{Read, Write};

use base64::Engine as _;
use materialcraft_core::{
    BaseParams, Comment, Connection, ExposedParam, Frame, Graph, MAX_SIZE_LOG2, MIN_SIZE_LOG2, MISSING_NODE, Node, NodeId, OutputFormat, Package, Resource,
    ResourceBytes, SizeMode, Value, linear_to_srgb, missing_ports, srgb_to_linear,
};
use materialcraft_nodes::{PortKind, find};
use roxmltree::Node as XmlNode;

use crate::{FormatError, Result};

#[path = "sbs_functions.rs"]
mod functions;

/// The SBS package extension.
pub const SBS_EXT: &str = "sbs";

const MAX_SBS_BYTES: usize = 256 << 20;
const MAX_EMBEDDED_BYTES: usize = 256 << 20;
const MAX_XML_NODES: u32 = 8_000_000;
const MAX_GRAPHS: usize = 4096;
const MAX_NODES: usize = 100_000;
const MAX_RESOURCES: usize = 4096;
/// Distinct warning lines kept per read; the rest are counted.
const MAX_WARNINGS: usize = 600;
/// Size of the parsed head [`is_sbs`] sniffs.
const SNIFF_BYTES: usize = 4096;

/// Marks the exact MaterialCraft copy at the end of a written package.
const EXACT_MARK: &str = "materialcraft-package";
const EXACT_VERSION: u32 = 1;
/// Dependency file name of MaterialCraft library nodes written as instances.
const LIBRARY_DEPENDENCY: &str = "materialcraft://library.sbs";
/// The dependency file name a package uses for itself.
const SELF_DEPENDENCY: &str = "?himself";
const FORMAT_VERSION: &str = "1.1.0.202401";
/// The first uid the writer hands out (any value works; large ones stay clear of small hand-written uids).
const FIRST_UID: u64 = 1_000_000_000;

/// Offset between an SBS node position (its center) and MaterialCraft's (its top-left corner), in graph units.
const NODE_HALF_SIZE: [f32; 2] = [50.0, 40.0];

/// A read SBS package.
#[derive(Debug)]
pub struct SbsRead {
    pub pkg: Package,
    /// What could not be read exactly (missing nodes, unsupported parameters, unavailable resources).
    pub warnings: Vec<String>,
    /// Whether the package came from MaterialCraft's exact copy, so nothing was lost.
    pub exact: bool,
}

/// Whether `bytes` look like an SBS package: XML whose root element is `<package>`.
pub fn is_sbs(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(SNIFF_BYTES)]);
    let mut rest = head.trim_start_matches('\u{feff}');

    // Skip the XML declaration and comments before the root element
    loop {
        rest = rest.trim_start();
        let (skip_open, skip_close) = if rest.starts_with("<?") {
            ("<?", "?>")
        } else if rest.starts_with("<!--") {
            ("<!--", "-->")
        } else {
            break;
        };
        match rest[skip_open.len()..].find(skip_close) {
            Some(end) => rest = &rest[skip_open.len() + end + skip_close.len()..],
            None => return false,
        }
    }

    rest.strip_prefix("<package").is_some_and(|after| after.starts_with(|c: char| c == '>' || c == '/' || c.is_whitespace()))
}

// ---------------------------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------------------------

/// Warning lines, deduplicated with counts, in first-seen order.
#[derive(Default)]
struct Warnings {
    lines: Vec<(String, usize)>,
    line_index: HashMap<String, usize>,
}

impl Warnings {
    fn add(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(line) = self.line_index.get(&message).and_then(|&i| self.lines.get_mut(i)) {
            line.1 += 1;
            return;
        }

        self.line_index.insert(message.clone(), self.lines.len());
        self.lines.push((message, 1));
    }

    fn finish(self) -> Vec<String> {
        let total = self.lines.len();
        let mut out: Vec<String> =
            self.lines.into_iter().take(MAX_WARNINGS).map(|(line, count)| if count > 1 { format!("{line} (×{count})") } else { line }).collect();
        if total > MAX_WARNINGS {
            out.push(format!("…and {} more warnings", total - MAX_WARNINGS));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Atomic filters
// ---------------------------------------------------------------------------------------------

/// How an SBS parameter becomes a MaterialCraft one.
#[derive(Clone, Copy, PartialEq)]
enum ParamConversion {
    /// By the MaterialCraft parameter's type.
    ByType,
    /// An SBS boolean that is `true` for option 0 of a two-option MaterialCraft choice.
    TrueIsFirstOption,
}

use ParamConversion::{ByType, TrueIsFirstOption};

/// An SBS atomic filter and the MaterialCraft atomic node it maps to.
struct FilterMap {
    sbs: &'static str,
    def: &'static str,
    /// (SBS input identifier, MaterialCraft input port).
    inputs: &'static [(&'static str, &'static str)],
    /// (SBS parameter, MaterialCraft parameter, conversion).
    params: &'static [(&'static str, &'static str, ParamConversion)],
}

const FILTERS: &[FilterMap] = &[
    FilterMap { sbs: "uniform", def: "uniform", inputs: &[], params: &[("outputcolor", "color", ByType), ("colorswitch", "mode", TrueIsFirstOption)] },
    FilterMap {
        sbs: "blend",
        def: "blend",
        inputs: &[("source", "foreground"), ("destination", "background"), ("opacity", "mask")],
        params: &[("blendingmode", "mode", ByType), ("opacitymult", "opacity", ByType)],
    },
    FilterMap { sbs: "blur", def: "blur", inputs: &[("input1", "input")], params: &[("intensity", "intensity", ByType)] },
    FilterMap {
        sbs: "shuffle",
        def: "channels_shuffle",
        inputs: &[("input1", "input1"), ("input2", "input2")],
        params: &[("channelred", "red", ByType), ("channelgreen", "green", ByType), ("channelblue", "blue", ByType), ("channelalpha", "alpha", ByType)],
    },
    FilterMap { sbs: "curve", def: "curve", inputs: &[("input1", "input")], params: &[] },
    FilterMap {
        sbs: "dirmotionblur",
        def: "directional_blur",
        inputs: &[("input1", "input")],
        params: &[("intensity", "intensity", ByType), ("mblurangle", "angle", ByType)],
    },
    FilterMap {
        sbs: "directionalwarp",
        def: "directional_warp",
        inputs: &[("input1", "input"), ("inputintensity", "intensity_input")],
        params: &[("intensity", "intensity", ByType), ("warpangle", "angle", ByType)],
    },
    FilterMap {
        sbs: "distance",
        def: "distance",
        inputs: &[("mask", "mask"), ("source", "source")],
        params: &[("distance", "distance", ByType), ("combinedistance", "combine", ByType)],
    },
    FilterMap {
        sbs: "emboss",
        def: "emboss",
        inputs: &[("input1", "input"), ("inputgradient", "height")],
        params: &[
            ("intensity", "intensity", ByType),
            ("lightangle", "light_angle", ByType),
            ("highlightcolor", "highlight", ByType),
            ("shadowcolor", "shadow", ByType),
        ],
    },
    FilterMap {
        sbs: "dyngradient",
        def: "gradient_dynamic",
        inputs: &[("input1", "input"), ("input2", "gradient"), ("inputgradient", "gradient")],
        params: &[("position", "position", ByType), ("orientation", "orientation", ByType)],
    },
    FilterMap { sbs: "gradient", def: "gradient_map", inputs: &[("input1", "input")], params: &[("colorswitch", "mode", TrueIsFirstOption)] },
    FilterMap { sbs: "grayscaleconversion", def: "grayscale_conversion", inputs: &[("input1", "input")], params: &[] },
    FilterMap {
        sbs: "hsl",
        def: "hsl",
        inputs: &[("input1", "input")],
        params: &[("hue", "hue", ByType), ("saturation", "saturation", ByType), ("luminance", "lightness", ByType)],
    },
    FilterMap { sbs: "bitmap", def: "bitmap", inputs: &[], params: &[] },
    FilterMap {
        sbs: "levels",
        def: "levels",
        inputs: &[("input1", "input")],
        params: &[
            ("levelinlow", "in_low", ByType),
            ("levelinmid", "in_mid", ByType),
            ("levelinhigh", "in_high", ByType),
            ("leveloutlow", "out_low", ByType),
            ("levelouthigh", "out_high", ByType),
        ],
    },
    FilterMap {
        sbs: "normal",
        def: "normal",
        inputs: &[("input1", "input")],
        params: &[("intensity", "intensity", ByType), ("inversedy", "format", TrueIsFirstOption), ("input2alpha", "height_alpha", ByType)],
    },
    FilterMap {
        sbs: "pixelprocessor",
        def: "pixel_processor",
        inputs: &[("input", "input0"), ("input:1", "input1"), ("input:2", "input2"), ("input:3", "input3")],
        params: &[],
    },
    FilterMap { sbs: "valueprocessor", def: "value_processor", inputs: &[("input", "input0")], params: &[] },
    FilterMap { sbs: "sharpen", def: "sharpen", inputs: &[("input1", "input")], params: &[("intensity", "intensity", ByType)] },
    FilterMap {
        sbs: "transformation",
        def: "transformation",
        inputs: &[("input1", "input")],
        params: &[("offset", "offset", ByType), ("mattecolor", "matte", ByType)],
    },
    FilterMap { sbs: "warp", def: "warp", inputs: &[("input1", "input"), ("inputgradient", "gradient")], params: &[("intensity", "intensity", ByType)] },
    FilterMap { sbs: "passthrough", def: "dot", inputs: &[("input", "input"), ("input1", "input")], params: &[] },
    FilterMap { sbs: "text", def: "text", inputs: &[], params: &[("text", "text", ByType)] },
    FilterMap { sbs: "svg", def: "svg", inputs: &[], params: &[] },
    FilterMap { sbs: "fxmaps", def: "fx_map", inputs: &[], params: &[] },
];

/// SBS parameters every node has, read as base parameters.
const BASE_PARAMS: &[&str] = &["outputsize", "format", "randomseed", "tiling", "pixelsize", "pixelratio", "quality"];

/// Filter parameters handled in code or knowingly ignored, so they raise no warning.
const HANDLED_PARAMS: &[(&str, &str)] = &[
    ("bitmap", "bitmapresourcepath"),
    ("bitmap", "colorspace"),
    ("grayscaleconversion", "channelsweights"),
    ("transformation", "matrix22"),
    ("curve", "exposecurve"),
];

fn filter_by_sbs_name(name: &str) -> Option<&'static FilterMap> {
    FILTERS.iter().find(|f| f.sbs == name)
}

fn filter_by_def(def: &str) -> Option<&'static FilterMap> {
    FILTERS.iter().find(|f| f.def == def)
}

/// The MaterialCraft usage index of an SBS usage name; unknown names are "any".
fn usage_index(name: &str) -> u32 {
    let ids = materialcraft_nodes::atomic::USAGE_IDS;
    let name = name.trim();
    let index = ids.iter().position(|u| u.eq_ignore_ascii_case(name)).or_else(|| name.eq_ignore_ascii_case("diffuse").then_some(0));
    index.unwrap_or(ids.len().saturating_sub(1)) as u32
}

fn usage_name(index: u32) -> &'static str {
    materialcraft_nodes::atomic::USAGE_IDS.get(index as usize).copied().unwrap_or("any")
}

// ---------------------------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------------------------

/// A constant as SBS stores it.
#[derive(Clone, Debug, PartialEq)]
enum SbsValue {
    Floats(Vec<f32>),
    Ints(Vec<i32>),
    Bool(bool),
    Text(String),
}

/// Up to 16 whitespace-separated floats; bad or non-finite ones read as 0.
fn parse_floats(text: &str) -> Vec<f32> {
    text.split_whitespace().take(16).map(|t| t.parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(0.0)).collect()
}

/// Up to 16 whitespace-separated integers, saturated to `i32`; bad ones read as 0.
fn parse_ints(text: &str) -> Vec<i32> {
    let parse = |t: &str| t.parse::<i64>().map(|v| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32).or_else(|_| t.parse::<f64>().map(|v| v as i32));
    text.split_whitespace().take(16).map(|t| parse(t).unwrap_or(0)).collect()
}

/// The constant under `holder` (a `paramValue`, `defaultValue` or `constValue` element).
fn sbs_constant(holder: XmlNode) -> Option<SbsValue> {
    let element = holder.children().find(|c| c.is_element())?;
    sbs_constant_element(element)
}

fn sbs_constant_element(element: XmlNode) -> Option<SbsValue> {
    let text = element.attribute("v").unwrap_or("");
    let tag = element.tag_name().name();

    if tag == "constantValueBool" {
        Some(SbsValue::Bool(parse_ints(text).first().is_some_and(|b| *b != 0) || text.trim().eq_ignore_ascii_case("true")))
    } else if tag == "constantValueString" {
        Some(SbsValue::Text(text.to_string()))
    } else if tag.starts_with("constantValueFloat") {
        Some(SbsValue::Floats(parse_floats(text)))
    } else if tag.starts_with("constantValueInt") {
        Some(SbsValue::Ints(parse_ints(text)))
    } else {
        None
    }
}

impl SbsValue {
    /// Component `i` as a float (the first component when there are fewer).
    fn float(&self, i: usize) -> f32 {
        match self {
            SbsValue::Floats(v) => v.get(i).or(v.first()).copied().unwrap_or(0.0),
            SbsValue::Ints(v) => v.get(i).or(v.first()).copied().unwrap_or(0) as f32,
            SbsValue::Bool(b) => f32::from(u8::from(*b)),
            SbsValue::Text(_) => 0.0,
        }
    }

    /// Component `i` as an integer (the first component when there are fewer).
    fn int(&self, i: usize) -> i32 {
        match self {
            SbsValue::Ints(v) => v.get(i).or(v.first()).copied().unwrap_or(0),
            SbsValue::Floats(v) => v.get(i).or(v.first()).copied().unwrap_or(0.0).round() as i32,
            SbsValue::Bool(b) => i32::from(*b),
            SbsValue::Text(_) => 0,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            SbsValue::Bool(b) => *b,
            SbsValue::Text(s) => s == "1" || s.eq_ignore_ascii_case("true"),
            _ => self.float(0) != 0.0,
        }
    }

    fn component_count(&self) -> usize {
        match self {
            SbsValue::Floats(v) => v.len(),
            SbsValue::Ints(v) => v.len(),
            _ => 1,
        }
    }
}

/// `value` as the type of `like` (a parameter default); `options` is an enum's option count.
fn to_param_value(like: &Value, value: &SbsValue, options: usize) -> Option<Value> {
    let f = |i| value.float(i);

    Some(match like {
        Value::Float(_) => Value::Float(f(0)),
        Value::Float2(_) => Value::Float2([f(0), f(1)]),
        Value::Float3(_) => Value::Float3([f(0), f(1), f(2)]),
        Value::Float4(_) => Value::Float4([f(0), f(1), f(2), f(3)]),
        // SBS colors are stored as displayed (sRGB); MaterialCraft's are linear
        Value::Color(_) if value.component_count() >= 3 => {
            let alpha = if value.component_count() >= 4 { f(3) } else { 1.0 };
            Value::Color([srgb_to_linear(f(0)), srgb_to_linear(f(1)), srgb_to_linear(f(2)), alpha])
        }
        Value::Color(_) => {
            let gray = srgb_to_linear(f(0));
            Value::Color([gray, gray, gray, 1.0])
        }
        Value::Int(_) => Value::Int(value.int(0)),
        Value::Int2(_) => Value::Int2([value.int(0), value.int(1)]),
        Value::Bool(_) => Value::Bool(value.truthy()),
        Value::Enum(_) => {
            let last = options.saturating_sub(1).min(i32::MAX as usize) as i32;
            Value::Enum(value.int(0).clamp(0, last) as u32)
        }
        Value::Text(_) => match value {
            SbsValue::Text(s) => Value::Text(s.clone()),
            _ => return None,
        },
        Value::Curve(_) | Value::Gradient(_) | Value::Function(_) => return None,
    })
}

/// `value` as the MaterialCraft value of the same shape (for exposed inputs and instance parameters).
fn to_natural_value(value: &SbsValue) -> Value {
    let f = |i| value.float(i);

    match value {
        SbsValue::Bool(b) => Value::Bool(*b),
        SbsValue::Text(s) => Value::Text(s.clone()),
        SbsValue::Ints(v) if v.len() <= 1 => Value::Int(value.int(0)),
        SbsValue::Ints(_) => Value::Int2([value.int(0), value.int(1)]),
        SbsValue::Floats(v) => match v.len() {
            0 | 1 => Value::Float(f(0)),
            2 => Value::Float2([f(0), f(1)]),
            3 => Value::Float3([f(0), f(1), f(2)]),
            _ => Value::Float4([f(0), f(1), f(2), f(3)]),
        },
    }
}

/// SBS type codes of exposed graph inputs.
mod type_code {
    pub const COLOR_IMAGE: i64 = 1;
    pub const GRAY_IMAGE: i64 = 2;
    pub const BOOL: i64 = 4;
    pub const INT1: i64 = 16;
    pub const INT2: i64 = 32;
    pub const INT3: i64 = 64;
    pub const INT4: i64 = 128;
    pub const FLOAT1: i64 = 256;
    pub const FLOAT2: i64 = 512;
    pub const FLOAT3: i64 = 1024;
    pub const FLOAT4: i64 = 2048;
    pub const STRING: i64 = 16384;
}

/// An exposed input's default from its SBS type code.
fn exposed_default(code: i64, value: Option<&SbsValue>) -> Value {
    use type_code::*;

    let value = value.cloned().unwrap_or(SbsValue::Floats(vec![0.0]));
    let f = |i| value.float(i);
    match code {
        BOOL => Value::Bool(value.truthy()),
        INT1 => Value::Int(value.int(0)),
        INT2 | INT3 | INT4 => Value::Int2([value.int(0), value.int(1)]),
        FLOAT1 => Value::Float(f(0)),
        FLOAT2 => Value::Float2([f(0), f(1)]),
        FLOAT3 => Value::Float3([f(0), f(1), f(2)]),
        FLOAT4 => Value::Float4([f(0), f(1), f(2), f(3)]),
        STRING => match &value {
            SbsValue::Text(s) => Value::Text(s.clone()),
            _ => Value::Text(String::new()),
        },
        _ => to_natural_value(&value),
    }
}

fn exposed_type_code(value: &Value) -> i64 {
    use type_code::*;

    match value {
        Value::Bool(_) => BOOL,
        Value::Int(_) | Value::Enum(_) => INT1,
        Value::Int2(_) => INT2,
        Value::Float(_) | Value::Curve(_) | Value::Gradient(_) | Value::Function(_) => FLOAT1,
        Value::Float2(_) => FLOAT2,
        Value::Float3(_) => FLOAT3,
        Value::Float4(_) | Value::Color(_) => FLOAT4,
        Value::Text(_) => STRING,
    }
}

/// `v` for XML, with non-finite values written as 0.
fn format_float(v: f32) -> String {
    if v.is_finite() { format!("{v}") } else { "0".into() }
}

/// The SBS constant element for `value` (`None` for curves, gradients and functions).
fn constant_xml(value: &Value) -> Option<String> {
    let join = |xs: &[f32]| xs.iter().map(|x| format_float(*x)).collect::<Vec<_>>().join(" ");
    let (tag, text) = match value {
        Value::Float(x) => ("constantValueFloat1", format_float(*x)),
        Value::Float2(x) => ("constantValueFloat2", join(x)),
        Value::Float3(x) => ("constantValueFloat3", join(x)),
        Value::Float4(x) => ("constantValueFloat4", join(x)),
        Value::Color(c) => ("constantValueFloat4", join(&[linear_to_srgb(c[0]), linear_to_srgb(c[1]), linear_to_srgb(c[2]), c[3]])),
        Value::Int(i) => ("constantValueInt32", i.to_string()),
        Value::Int2(i) => ("constantValueInt2", format!("{} {}", i[0], i[1])),
        Value::Enum(e) => ("constantValueInt32", e.to_string()),
        Value::Bool(b) => ("constantValueBool", u8::from(*b).to_string()),
        Value::Text(s) => ("constantValueString", s.clone()),
        Value::Curve(_) | Value::Gradient(_) | Value::Function(_) => return None,
    };

    Some(format!("<{tag} v=\"{}\"/>", xml_escape(&text)))
}

// ---------------------------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------------------------

fn child_element<'a, 'input>(node: XmlNode<'a, 'input>, tag: &str) -> Option<XmlNode<'a, 'input>> {
    node.children().find(|c| c.is_element() && c.tag_name().name() == tag)
}

fn children_named<'a, 'input: 'a>(node: XmlNode<'a, 'input>, tag: &'a str) -> impl Iterator<Item = XmlNode<'a, 'input>> + 'a {
    node.children().filter(move |c| c.is_element() && c.tag_name().name() == tag)
}

/// The `v` attribute of child element `tag`, how SBS stores every scalar.
fn child_value<'a>(node: XmlNode<'a, '_>, tag: &str) -> Option<&'a str> {
    child_element(node, tag).and_then(|c| c.attribute("v"))
}

/// `text` escaped for XML attributes and text, without characters XML 1.0 can't hold.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// FNV-1a (64 bits) of the `<content>` an exact copy was written with.
fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, b| (hash ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn base64_engine() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Decode embedded data: base64 of a big-endian u32 length followed by zlib-compressed bytes.
fn decode_embedded(strdata: &str) -> std::result::Result<Vec<u8>, String> {
    let raw = base64_engine().decode(strdata.split_whitespace().collect::<String>()).map_err(|e| format!("bad base64: {e}"))?;
    let (length, compressed) = raw.split_at_checked(4).ok_or("too short")?;
    let length = u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize;
    if length > MAX_EMBEDDED_BYTES {
        return Err(format!("{length} bytes is over the {MAX_EMBEDDED_BYTES}-byte limit"));
    }

    let mut out = Vec::with_capacity(length.min(1 << 24));
    let mut decoder = flate2::read::ZlibDecoder::new(compressed).take(MAX_EMBEDDED_BYTES as u64 + 1);
    decoder.read_to_end(&mut out).map_err(|e| format!("bad compressed data: {e}"))?;
    if out.len() != length {
        return Err(format!("{} bytes where {length} were declared", out.len()));
    }
    Ok(out)
}

fn encode_embedded(bytes: &[u8]) -> Result<String> {
    let length = u32::try_from(bytes.len()).map_err(|_| FormatError::Io("resource too large".into()))?;
    let mut encoder = flate2::write::ZlibEncoder::new(length.to_be_bytes().to_vec(), flate2::Compression::default());
    encoder.write_all(bytes).map_err(|e| FormatError::Io(e.to_string()))?;
    let raw = encoder.finish().map_err(|e| FormatError::Io(e.to_string()))?;
    Ok(base64_engine().encode(raw))
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Read an SBS package; `name` is its file name and `linked` loads files it links to by relative path.
pub fn read_sbs(bytes: &[u8], name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<SbsRead> {
    if bytes.len() > MAX_SBS_BYTES {
        return Err(FormatError::BadPackage(format!("the SBS file is over the {} MB limit", MAX_SBS_BYTES >> 20)));
    }

    let text = std::str::from_utf8(bytes).map_err(|e| FormatError::BadPackage(format!("the SBS file is not UTF-8 text: {e}")))?;
    let text = text.trim_start_matches('\u{feff}');
    let options = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: MAX_XML_NODES };
    let doc = roxmltree::Document::parse_with_options(text, options).map_err(|e| FormatError::BadPackage(format!("the SBS file is not valid XML: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "package" {
        return Err(FormatError::BadPackage("not an SBS package (the root element is not <package>)".into()));
    }

    let mut warnings = Warnings::default();
    let file_stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let file_stem = file_stem.rsplit_once('.').map_or(file_stem, |(stem, _)| stem).trim();
    let package_name = match child_value(root, "identifier").map(str::trim) {
        Some(id) if !id.is_empty() && id != "Unsaved Package" => id.to_string(),
        _ if !file_stem.is_empty() => file_stem.to_string(),
        _ => "Untitled package".to_string(),
    };
    let mut reader = Reader {
        text,
        warnings: &mut warnings,
        dependencies: HashMap::new(),
        graph_names: HashMap::new(),
        resource_names: HashMap::new(),
        node_count: 0,
        function_variables: BTreeMap::new(),
    };
    for dependency in child_element(root, "dependencies").into_iter().flat_map(|d| children_named(d, "dependency")) {
        if let (Some(uid), Some(file)) = (child_value(dependency, "uid"), child_value(dependency, "filename")) {
            reader.dependencies.insert(uid.to_string(), file.to_string());
        }
    }

    // Resources first: the exact copy and the SBS graphs both use them
    let content = child_element(root, "content");
    let mut pkg = Package::new(&package_name);
    let mut graph_elements: Vec<(String, XmlNode)> = Vec::new();
    if let Some(content) = content {
        reader.collect_items(content, "", &mut pkg, &mut graph_elements, linked);
    }

    if let Some(content) = content
        && let Some(mut exact) = read_exact_copy(root, &text[content.range()], reader.warnings)
    {
        for resource in &mut exact.resources {
            match pkg.resources.iter().find(|r| r.name == resource.name) {
                Some(found) => resource.bytes = found.bytes.clone(),
                None => reader.warnings.add(format!("resource `{}` is missing from the file", resource.name)),
            }
        }
        report_placeholders(&exact, reader.warnings);
        return Ok(SbsRead { pkg: exact, warnings: warnings.finish(), exact: true });
    }

    // Name every graph first so instances can refer to graphs defined later in the file
    let mut names: Vec<String> = Vec::new();
    for (path, graph) in graph_elements.iter().take(MAX_GRAPHS) {
        let identifier = child_value(*graph, "identifier").unwrap_or("").trim();
        let unique = unique_name(&names, if identifier.is_empty() { "Graph" } else { identifier });
        reader.graph_names.insert(path.clone(), unique.clone());
        names.push(unique);
    }
    if graph_elements.len() > MAX_GRAPHS {
        reader.warnings.add(format!("only the first {MAX_GRAPHS} graphs were read"));
    }

    for ((_, graph), name) in graph_elements.iter().zip(names) {
        let graph = reader.read_graph(*graph, name);
        pkg.graphs.push(graph);
    }
    if pkg.graphs.is_empty() {
        reader.warnings.add("the package has no graphs MaterialCraft can open; an empty graph was added");
        pkg.graphs.push(Graph::new("Graph"));
    }

    report_placeholders(&pkg, reader.warnings);
    Ok(SbsRead { pkg, warnings: warnings.finish(), exact: false })
}

/// One warning per missing node kind and graph.
fn report_placeholders(pkg: &Package, warnings: &mut Warnings) {
    for graph in &pkg.graphs {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for node in graph.nodes.iter().filter(|n| n.def == MISSING_NODE) {
            let name = match node.params.get("sbs_name") {
                Some(Value::Text(s)) if !s.is_empty() => s.clone(),
                _ => node.label.clone(),
            };
            *counts.entry(name).or_default() += 1;
        }

        for (name, count) in counts {
            let times = if count > 1 { format!(" ×{count}") } else { String::new() };
            warnings.add(format!("graph `{}`: missing node `{name}`{times} is not in MaterialCraft's library yet; kept as a placeholder", graph.name));
        }
    }
}

/// The exact MaterialCraft copy, if present and written for exactly this `content`.
fn read_exact_copy(root: XmlNode, content: &str, warnings: &mut Warnings) -> Option<Package> {
    let comment = root.children().filter(|c| c.is_comment()).filter_map(|c| c.text()).find(|t| t.trim_start().starts_with(EXACT_MARK))?;
    let field = |key: &str| comment.split_whitespace().find_map(|part| part.strip_prefix(key));

    if field("version=").and_then(|v| v.parse::<u32>().ok()) != Some(EXACT_VERSION) {
        warnings.add("the MaterialCraft data in this file is from another version; the graphs were read from the SBS data");
        return None;
    }
    if field("hash=") != Some(format!("{:016x}", content_hash(content.as_bytes())).as_str()) {
        warnings.add("this file was changed outside MaterialCraft since it was saved; the graphs were read from the SBS data");
        return None;
    }

    let decoded = decode_embedded(field("data=").unwrap_or("")).and_then(|json| serde_json::from_slice::<Package>(&json).map_err(|e| e.to_string()));
    decoded.map_err(|e| warnings.add(format!("the MaterialCraft data in this file is damaged ({e}); the graphs were read from the SBS data"))).ok()
}

fn unique_name(taken: &[String], base: &str) -> String {
    let free = |name: &str| !taken.iter().any(|t| t == name);
    if free(base) {
        return base.to_string();
    }
    (1..).map(|i| format!("{base}_{i}")).find(|name| free(name)).unwrap_or_else(|| base.to_string())
}

struct Reader<'text, 'warnings> {
    text: &'text str,
    warnings: &'warnings mut Warnings,
    /// Dependency uid → file name.
    dependencies: HashMap<String, String>,
    /// Package path (`Folder/Graph`) → graph name.
    graph_names: HashMap<String, String>,
    /// Package path (`Resources/Bitmap`) → resource name.
    resource_names: HashMap<String, String>,
    node_count: usize,
    /// Exposed numeric defaults of the graph currently being read.
    function_variables: BTreeMap<String, Value>,
}

/// A node being read, as wires from the SBS data find it.
struct ReadNode {
    id: NodeId,
    /// Position in `Graph::nodes`.
    index: usize,
    /// SBS input identifier → MaterialCraft port.
    inputs: HashMap<String, String>,
    /// (SBS output uid, MaterialCraft port), in output order.
    outputs: Vec<(String, String)>,
    /// Placeholders and graph instances take whatever inputs wires arrive at.
    open_inputs: bool,
}

/// Add a node and return it with its position in `graph.nodes`.
fn push_node<'g>(graph: &'g mut Graph, def: &str, pos: [f32; 2]) -> (NodeId, usize, Option<&'g mut Node>) {
    let id = graph.add_node(def, pos);
    let index = graph.nodes.len() - 1;
    (id, index, graph.nodes.last_mut())
}

impl<'text> Reader<'text, '_> {
    /// Walk `content` and nested groups: queue graphs, load resources, note what is skipped.
    fn collect_items<'a>(
        &mut self,
        content: XmlNode<'a, 'text>,
        folder: &str,
        pkg: &mut Package,
        graphs: &mut Vec<(String, XmlNode<'a, 'text>)>,
        linked: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) {
        for item in content.children().filter(|c| c.is_element()) {
            let identifier = child_value(item, "identifier").unwrap_or("").trim().to_string();
            let path = if folder.is_empty() { identifier.clone() } else { format!("{folder}/{identifier}") };

            match item.tag_name().name() {
                "graph" => graphs.push((path, item)),
                "group" => {
                    if let Some(group_content) = child_element(item, "content") {
                        self.collect_items(group_content, &path, pkg, graphs, linked);
                    }
                }
                "resource" if pkg.resources.len() >= MAX_RESOURCES => self.warnings.add(format!("only the first {MAX_RESOURCES} resources were read")),
                "resource" => self.read_resource(item, &identifier, &path, pkg, linked),
                "function" => self.warnings.add(format!("function graph `{path}` is not imported yet (SBS function graphs)")),
                "mdl_graph" | "mdlGraph" => self.warnings.add(format!("MDL graph `{path}` is not supported")),
                other => self.warnings.add(format!("package item <{other}> `{path}` is not supported")),
            }
        }
    }

    fn read_resource(&mut self, item: XmlNode, identifier: &str, path: &str, pkg: &mut Package, linked: &dyn Fn(&str) -> Option<Vec<u8>>) {
        let kind = child_value(item, "type").unwrap_or("bitmap");
        let format = child_value(item, "format").unwrap_or("png").trim().to_ascii_lowercase();
        let source = child_element(item, "source");
        let mut bytes: Option<Vec<u8>> = None;
        let mut link: Option<String> = None;
        let mut why_missing = String::new();

        if let Some(embedded) = source.and_then(|s| child_element(s, "binembedded")) {
            match decode_embedded(child_value(embedded, "strdata").unwrap_or("")) {
                Ok(data) => bytes = Some(data),
                Err(e) => why_missing = format!("its embedded data is damaged ({e})"),
            }
        }

        if bytes.is_none() {
            let external_copy = source.and_then(|s| child_element(s, "externalcopy")).and_then(|e| child_value(e, "filename"));
            for file in [external_copy, child_value(item, "filepath")].into_iter().flatten().filter(|p| !p.trim().is_empty()) {
                if file.contains("://") {
                    why_missing = format!("it links to `{file}`, which is not available");
                    continue;
                }
                match linked(file) {
                    Some(data) => {
                        bytes = Some(data);
                        link = Some(file.to_string());
                        break;
                    }
                    None => why_missing = format!("its file `{file}` was not found next to the package"),
                }
            }
        }

        let taken: Vec<String> = pkg.resources.iter().map(|r| r.name.clone()).collect();
        let name = unique_name(&taken, if identifier.is_empty() { "resource" } else { identifier });
        self.resource_names.insert(path.to_string(), name.clone());
        match bytes {
            Some(data) => {
                let ext = if kind == "svg" { "svg".to_string() } else { format };
                let file = crate::safe_file_name(&format!("{name}.{ext}"));
                pkg.resources.push(match link {
                    Some(l) => Resource::linked(name.clone(), file, ResourceBytes::new(data), l),
                    None => Resource::new(name.clone(), file, ResourceBytes::new(data)),
                });
            }
            None => {
                let why = if why_missing.is_empty() { "it has no data".to_string() } else { why_missing };
                self.warnings.add(format!("resource `{path}` could not be loaded: {why}"));
            }
        }
    }

    /// Split a `pkg:///path?dependency=uid` reference into the path and the dependency's file name.
    fn resolve_reference(&self, url: &str) -> (String, Option<String>) {
        let url = url.trim();
        let rest = url.strip_prefix("pkg:///").or_else(|| url.strip_prefix("pkg://")).unwrap_or(url);
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let dependency = query.split('&').find_map(|kv| kv.strip_prefix("dependency=")).and_then(|uid| self.dependencies.get(uid).cloned());
        (path.trim_matches('/').to_string(), dependency)
    }

    fn read_graph(&mut self, element: XmlNode<'_, 'text>, name: String) -> Graph {
        let mut graph = Graph::new(&name);

        // The default parent size option ("11x11", log2 sizes)
        for option in child_element(element, "options").into_iter().flat_map(|o| children_named(o, "option")) {
            if child_value(option, "name") == Some("defaultParentSize")
                && let Some((width, height)) = child_value(option, "value").and_then(|v| v.split_once('x'))
            {
                let log2 = |s: &str| s.trim().parse::<i64>().unwrap_or(10).clamp(i64::from(MIN_SIZE_LOG2), i64::from(MAX_SIZE_LOG2)) as u32;
                graph.size = [log2(width), log2(height)];
            }
        }

        if let Some(attributes) = child_element(element, "attributes") {
            let text = |tag: &str| child_value(attributes, tag).unwrap_or("").to_string();
            graph.attributes.category = text("category");
            graph.attributes.description = text("description");
            graph.attributes.author = text("author");
            graph.attributes.tags = text("tags");
        }

        let image_inputs = self.read_graph_inputs(element, &mut graph);
        self.function_variables = graph.exposed.iter().map(|p| (p.id.clone(), p.default.clone())).collect();
        let outputs = read_graph_outputs(element);

        let mut read_nodes: HashMap<String, ReadNode> = HashMap::new();
        let comp_nodes: Vec<XmlNode> = child_element(element, "compNodes").into_iter().flat_map(|c| children_named(c, "compNode")).collect();
        for comp_node in &comp_nodes {
            if self.node_count >= MAX_NODES {
                self.warnings.add(format!("only the first {MAX_NODES} nodes were read"));
                break;
            }

            let Some(uid) = child_value(*comp_node, "uid").map(str::to_string) else { continue };
            if read_nodes.contains_key(&uid) {
                self.warnings.add(format!("graph `{name}`: duplicate node uid {uid} skipped"));
                continue;
            }

            self.node_count += 1;
            let node = self.read_node(&mut graph, *comp_node, &image_inputs, &outputs);
            read_nodes.insert(uid, node);
        }

        let wires = self.read_wires(&mut graph, &comp_nodes, &read_nodes);
        connect_all(&mut graph, wires, self.warnings);
        self.read_canvas_items(element, &mut graph);
        graph
    }

    /// Exposed value inputs go into `graph`; image inputs are returned as uid → (identifier, gray).
    fn read_graph_inputs(&mut self, element: XmlNode, graph: &mut Graph) -> HashMap<String, (String, bool)> {
        let mut image_inputs = HashMap::new();
        for input in child_element(element, "paraminputs").into_iter().flat_map(|p| children_named(p, "paraminput")) {
            let id = child_value(input, "identifier").unwrap_or("input").to_string();
            let code = child_value(input, "type").and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(type_code::FLOAT1);
            if code == type_code::COLOR_IMAGE || code == type_code::GRAY_IMAGE {
                image_inputs.insert(child_value(input, "uid").unwrap_or("").to_string(), (id, code == type_code::GRAY_IMAGE));
                continue;
            }
            if graph.exposed.iter().any(|e| e.id == id) {
                continue;
            }

            let default = exposed_default(code, child_element(input, "defaultValue").and_then(sbs_constant).as_ref());
            let label = child_element(input, "attributes").and_then(|a| child_value(a, "label")).filter(|l| !l.is_empty()).unwrap_or(&id).to_string();
            let (mut min, mut max) = (0.0, 1.0);
            let widget_options = child_element(input, "defaultWidget").and_then(|w| child_element(w, "options"));
            for option in widget_options.into_iter().flat_map(|o| children_named(o, "option")) {
                let value =
                    child_value(option, "value").and_then(|v| v.split_whitespace().next()).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
                match (child_value(option, "name"), value) {
                    (Some("min"), Some(v)) => min = v,
                    (Some("max"), Some(v)) => max = v,
                    _ => {}
                }
            }

            let group = child_value(input, "group").unwrap_or("").to_string();
            graph.exposed.push(ExposedParam { id, label, group, default, min, max, options: Vec::new() });
        }
        image_inputs
    }

    /// Every wire of the graph, resolved to MaterialCraft nodes and ports, in file order.
    fn read_wires(&mut self, graph: &mut Graph, comp_nodes: &[XmlNode], read_nodes: &HashMap<String, ReadNode>) -> Vec<Connection> {
        let name = graph.name.clone();
        let mut wires = Vec::new();

        for comp_node in comp_nodes {
            let Some(to) = child_value(*comp_node, "uid").and_then(|u| read_nodes.get(u)) else { continue };
            for connection in child_element(*comp_node, "connections").into_iter().flat_map(|c| children_named(c, "connection")) {
                let sbs_input = child_value(connection, "identifier").unwrap_or("");
                let Some(from) = child_value(connection, "connRef").and_then(|u| read_nodes.get(u)) else {
                    self.warnings.add(format!("graph `{name}`: a wire from a node that is not in the file was dropped"));
                    continue;
                };

                let output_uid = child_value(connection, "connRefOutput").unwrap_or("");
                let from_port = from.outputs.iter().find(|(uid, _)| uid == output_uid).or(from.outputs.first()).map(|(_, port)| port.clone());
                let Some(from_port) = from_port else {
                    self.warnings.add(format!("graph `{name}`: a wire from a node with no outputs was dropped"));
                    continue;
                };

                let to_port = match to.inputs.get(sbs_input) {
                    Some(port) => port.clone(),
                    None if to.open_inputs => {
                        if let Some(node) = graph.nodes.get_mut(to.index) {
                            add_placeholder_input(node, sbs_input);
                        }
                        sbs_input.to_string()
                    }
                    None => {
                        let def = graph.nodes.get(to.index).map_or("", |n| n.def.as_str());
                        self.warnings.add(format!("graph `{name}`: `{def}` has no input `{sbs_input}`; its wire was dropped"));
                        continue;
                    }
                };
                wires.push(Connection { from: from.id, from_port, to: to.id, to_port });
            }
        }
        wires
    }

    /// Frames and comments.
    fn read_canvas_items(&mut self, element: XmlNode, graph: &mut Graph) {
        for item in child_element(element, "GUIObjects").into_iter().flat_map(|o| children_named(o, "GUIObject")) {
            let layout = child_element(item, "GUILayout");
            let pos = layout.and_then(|l| child_value(l, "gpos")).map(parse_floats).unwrap_or_default();
            let size = layout.and_then(|l| child_value(l, "size")).map(parse_floats).unwrap_or_default();
            let (x, y) = (pos.first().copied().unwrap_or(0.0), pos.get(1).copied().unwrap_or(0.0));
            let width = size.first().copied().unwrap_or(180.0).max(8.0);
            let height = size.get(1).copied().unwrap_or(70.0).max(8.0);
            let text = child_value(item, "GUIName").unwrap_or("").to_string();

            match child_value(item, "type") {
                Some("COMMENT") if child_value(item, "isFrameVisible") == Some("1") => {
                    let c = child_value(item, "frameColor").map(parse_floats).unwrap_or_default();
                    let color = if c.len() >= 4 { [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]] } else { [0.2, 0.4, 0.8, 0.25] };
                    let id = graph.alloc_id();
                    let title = child_value(item, "title").unwrap_or("").to_string();
                    graph.frames.push(Frame { id, title, description: text, rect: [x, y, width, height], color });
                }
                Some("COMMENT") => {
                    let id = graph.alloc_id();
                    graph.comments.push(Comment { id, text, pos: [x, y], size: [width, height] });
                }
                Some(other) => self.warnings.add(format!("graph `{}`: canvas item `{other}` is not supported", graph.name)),
                None => {}
            }
        }
    }

    /// Read one `compNode` into `graph`.
    fn read_node(
        &mut self,
        graph: &mut Graph,
        comp_node: XmlNode<'_, 'text>,
        image_inputs: &HashMap<String, (String, bool)>,
        outputs: &HashMap<String, (String, u32)>,
    ) -> ReadNode {
        let gpos = child_element(comp_node, "GUILayout").and_then(|l| child_value(l, "gpos")).map(parse_floats).unwrap_or_default();
        let pos = [gpos.first().copied().unwrap_or(0.0) - NODE_HALF_SIZE[0], gpos.get(1).copied().unwrap_or(0.0) - NODE_HALF_SIZE[1]];
        let output_uids: Vec<(String, bool)> = child_element(comp_node, "compOutputs")
            .into_iter()
            .flat_map(|o| children_named(o, "compOutput"))
            .map(|o| (child_value(o, "uid").unwrap_or("").to_string(), child_value(o, "comptype") == Some("2")))
            .collect();
        let all_outputs_to = |port: &str| output_uids.iter().map(|(uid, _)| (uid.clone(), port.to_string())).collect::<Vec<_>>();
        let text = self.text;

        let Some(implementation) = child_element(comp_node, "compImplementation").and_then(|i| i.children().find(|c| c.is_element())) else {
            let (id, index, node) = push_node(graph, MISSING_NODE, pos);
            if let Some(node) = node {
                fill_placeholder(node, "other", "empty node", "", None, &output_uids, None);
            }
            return ReadNode { id, index, inputs: HashMap::new(), outputs: Vec::new(), open_inputs: true };
        };

        let params = child_element(implementation, "parameters");
        match implementation.tag_name().name() {
            "compInputBridge" => {
                let entry = child_value(implementation, "entry").unwrap_or("");
                let fallback = || ("input".to_string(), output_uids.first().is_some_and(|(_, gray)| *gray));
                let (identifier, gray) = image_inputs.get(entry).cloned().unwrap_or_else(fallback);
                let (id, index, node) = push_node(graph, if gray { "input_grayscale" } else { "input_color" }, pos);
                if let Some(node) = node {
                    node.params.insert("identifier".into(), Value::Text(identifier));
                    node.base = read_base_params(params);
                }
                ReadNode { id, index, inputs: HashMap::new(), outputs: all_outputs_to("output"), open_inputs: false }
            }
            "compOutputBridge" => {
                let output = child_value(implementation, "output").unwrap_or("");
                let (identifier, usage) = outputs.get(output).cloned().unwrap_or_else(|| ("output".into(), usage_index("any")));
                let (id, index, node) = push_node(graph, "output", pos);
                if let Some(node) = node {
                    node.params.insert("identifier".into(), Value::Text(identifier));
                    node.params.insert("usage".into(), Value::Enum(usage));
                }
                let inputs = HashMap::from([("inputNodeOutput".to_string(), "input".to_string())]);
                ReadNode { id, index, inputs, outputs: Vec::new(), open_inputs: false }
            }
            "compFilter" => {
                let filter_name = child_value(implementation, "filter").unwrap_or("");
                let graph_name = graph.name.clone();
                match filter_by_sbs_name(filter_name).filter(|f| find(f.def).is_some()) {
                    Some(filter) => {
                        let (id, index, node) = push_node(graph, filter.def, pos);
                        if let Some(node) = node {
                            self.read_filter_params(node, filter, params, &graph_name);
                            node.base = read_base_params(params);
                        }
                        let inputs = filter.inputs.iter().map(|(sbs, port)| (sbs.to_string(), port.to_string())).collect();
                        ReadNode { id, index, inputs, outputs: all_outputs_to("output"), open_inputs: false }
                    }
                    None => {
                        let (id, index, node) = push_node(graph, MISSING_NODE, pos);
                        if let Some(node) = node {
                            fill_placeholder(node, "filter", filter_name, "", params.map(|p| &text[p.range()]), &output_uids, None);
                        }
                        ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(&output_uids, None), open_inputs: true }
                    }
                }
            }
            "compInstance" => self.read_instance(graph, implementation, pos, &output_uids),
            other => {
                let (id, index, node) = push_node(graph, MISSING_NODE, pos);
                if let Some(node) = node {
                    fill_placeholder(node, "other", other, "", None, &output_uids, None);
                }
                ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(&output_uids, None), open_inputs: true }
            }
        }
    }

    /// An instance: of a graph in this package, of a library node MaterialCraft has, or a placeholder.
    fn read_instance(&mut self, graph: &mut Graph, implementation: XmlNode<'_, 'text>, pos: [f32; 2], output_uids: &[(String, bool)]) -> ReadNode {
        let graph_name = graph.name.clone();
        let (path, dependency) = self.resolve_reference(child_value(implementation, "path").unwrap_or(""));
        let params = child_element(implementation, "parameters");
        let bridgings: Vec<(String, String)> = child_element(implementation, "outputBridgings")
            .into_iter()
            .flat_map(|b| children_named(b, "outputBridging"))
            .map(|b| (child_value(b, "uid").unwrap_or("").to_string(), child_value(b, "identifier").unwrap_or("output").to_string()))
            .collect();
        let instance_params =
            || params.into_iter().flat_map(|p| children_named(p, "parameter")).filter(|p| !BASE_PARAMS.contains(&child_value(*p, "name").unwrap_or("")));

        let own_graph = dependency.as_deref().is_none_or(|d| d == SELF_DEPENDENCY).then(|| self.graph_names.get(&path).cloned()).flatten();
        if let Some(sub_graph) = own_graph {
            let (id, index, node) = push_node(graph, "graph_instance", pos);
            if let Some(node) = node {
                node.params.insert("graph".into(), Value::Text(sub_graph));
                for param in instance_params() {
                    if let Some(value) = child_element(param, "paramValue").and_then(sbs_constant) {
                        node.params.insert(child_value(param, "name").unwrap_or("").to_string(), to_natural_value(&value));
                    }
                }
                node.base = read_base_params(params);
            }
            return ReadNode { id, index, inputs: HashMap::new(), outputs: bridgings, open_inputs: true };
        }

        let short_name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let library_def = find(&short_name).filter(|d| d.id != "graph_instance" && d.id != "output" && !d.id.starts_with("input_"));
        let Some(def) = library_def else {
            let (id, index, node) = push_node(graph, MISSING_NODE, pos);
            if let Some(node) = node {
                let raw_params = params.map(|p| &self.text[p.range()]);
                fill_placeholder(node, "instance", &short_name, dependency.as_deref().unwrap_or(""), raw_params, output_uids, Some(&bridgings));
                node.params.insert("sbs_path".into(), Value::Text(path));
            }
            return ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(output_uids, Some(&bridgings)), open_inputs: true };
        };

        let (id, index, node) = push_node(graph, def.id, pos);
        if let Some(node) = node {
            for param in instance_params() {
                let param_name = child_value(param, "name").unwrap_or("");
                let id = match (def.id, param_name) {
                    ("histogram_scan", "Position") => "position",
                    ("histogram_scan", "Contrast") => "contrast",
                    _ => param_name,
                };
                match def.param(id) {
                    Some(param_def) => self.set_param(node, id, &param_def.default, param_def.options.len(), param, &graph_name),
                    None => self.warnings.add(format!("`{}`: parameter `{param_name}` has no MaterialCraft equivalent and was ignored", def.id)),
                }
            }
            node.base = read_base_params(params);
        }

        let first_output = def.outputs.first().map_or("output", |o| o.id).to_string();
        let port_for = |identifier: &str| if def.output_index(identifier).is_some() { identifier.to_string() } else { first_output.clone() };
        let unbridged = output_uids.iter().filter(|(uid, _)| !bridgings.iter().any(|(b, _)| b == uid)).map(|(uid, _)| (uid.clone(), first_output.clone()));
        let outputs = bridgings.iter().map(|(uid, identifier)| (uid.clone(), port_for(identifier))).chain(unbridged).collect();
        let mut inputs: HashMap<_, _> = def.inputs.iter().map(|p| (p.id.to_string(), p.id.to_string())).collect();
        let input_aliases: &[(&str, &str)] = match def.id {
            "histogram_scan" => &[("Input_1", "input")],
            "auto_levels" => &[("Input", "input")],
            "flood_fill" => &[("mask", "input")],
            "height_blend" => &[("height_top", "top"), ("height_bottom", "bottom")],
            _ => &[],
        };
        inputs.extend(input_aliases.iter().map(|&(alias, port)| (alias.to_string(), port.to_string())));
        ReadNode { id, index, inputs, outputs, open_inputs: false }
    }

    /// Set MaterialCraft parameter `id` (default `like`) from the SBS `<parameter>` element `param`.
    fn set_param(&mut self, node: &mut Node, id: &str, like: &Value, options: usize, param: XmlNode, graph_name: &str) {
        let Some(param_value) = child_element(param, "paramValue") else { return };

        if let Some(value) = sbs_constant(param_value) {
            match to_param_value(like, &value, options) {
                Some(value) => {
                    node.params.insert(id.to_string(), value);
                }
                None => self.warnings.add(format!("`{}`: parameter `{id}` has a value type MaterialCraft can't read yet; kept the default", node.def)),
            }
            return;
        }

        // A parameter linked to a graph input is a function of one `get_*` node naming it
        let Some(dynamic) = child_element(param_value, "dynamicValue") else { return };
        let function_nodes: Vec<XmlNode> =
            child_element(dynamic, "paramNodes").into_iter().flat_map(|p| children_named(p, "paramNode")).take(functions::MAX_NODES + 1).collect();
        if let [only] = function_nodes.as_slice()
            && child_value(*only, "function").is_some_and(|f| f.starts_with("get_"))
            && let Some(input) = only.descendants().find(|c| c.tag_name().name() == "constantValueString").and_then(|c| c.attribute("v"))
        {
            node.bindings.insert(id.to_string(), input.to_string());
            return;
        }
        match functions::read(dynamic, &self.function_variables, like) {
            Ok(function) => {
                node.functions.insert(id.to_string(), function);
            }
            Err(reason) => {
                self.warnings.add(format!("graph `{graph_name}`: `{}` parameter `{id}` is driven by a function; imported its default ({reason})", node.def))
            }
        }
    }

    fn read_filter_params(&mut self, node: &mut Node, filter: &FilterMap, params: Option<XmlNode>, graph_name: &str) {
        let Some(def) = find(filter.def) else { return };

        let filter_params: Vec<XmlNode> =
            params.into_iter().flat_map(|p| children_named(p, "parameter")).filter(|p| !BASE_PARAMS.contains(&child_value(*p, "name").unwrap_or(""))).collect();
        for param in &filter_params {
            let param_name = child_value(*param, "name").unwrap_or("");
            let value = child_element(*param, "paramValue").and_then(sbs_constant);

            if let Some(&(_, mc_name, conversion)) = filter.params.iter().find(|(sbs, _, _)| *sbs == param_name) {
                let Some(param_def) = def.param(mc_name) else { continue };

                match (conversion, value) {
                    (TrueIsFirstOption, Some(v)) => {
                        node.params.insert(mc_name.into(), Value::Enum(if v.truthy() { 0 } else { 1 }));
                    }
                    (TrueIsFirstOption, None) => {}
                    (ByType, _) => self.set_param(node, mc_name, &param_def.default, param_def.options.len(), *param, graph_name),
                }
                continue;
            }

            match (filter.sbs, param_name, value) {
                ("bitmap", "bitmapresourcepath", Some(SbsValue::Text(url))) => {
                    let (path, _) = self.resolve_reference(&url);
                    match self.resource_names.get(&path) {
                        Some(name) => {
                            node.params.insert("resource".into(), Value::Text(name.clone()));
                        }
                        None => self.warnings.add(format!("graph `{graph_name}`: Bitmap uses resource `{path}`, which is not in the package")),
                    }
                }
                ("grayscaleconversion", "channelsweights", Some(weights)) => {
                    for (i, channel) in ["red", "green", "blue", "alpha"].into_iter().enumerate() {
                        node.params.insert(channel.into(), Value::Float(weights.float(i)));
                    }
                }
                ("transformation", "matrix22", Some(matrix)) => {
                    let (a, b, c, d) = (matrix.float(0), matrix.float(1), matrix.float(2), matrix.float(3));
                    let (scale_x, scale_y) = ((a * a + c * c).sqrt(), (b * b + d * d).sqrt());
                    if scale_x > 1e-6 && scale_y > 1e-6 {
                        node.params.insert("scale".into(), Value::Float2([scale_x, scale_y]));
                        node.params.insert("rotation".into(), Value::Float(c.atan2(a) / std::f32::consts::TAU));
                    }
                }
                _ if HANDLED_PARAMS.contains(&(filter.sbs, param_name)) => {}
                _ => self.warnings.add(format!("`{}`: SBS parameter `{param_name}` is not supported yet and was ignored", filter.def)),
            }
        }

        let not_imported = match filter.sbs {
            "curve" => Some("curve points"),
            "gradient" => Some("gradient keys"),
            "pixelprocessor" | "valueprocessor" => Some("function graph"),
            "svg" => Some("vector shapes"),
            "fxmaps" => Some("pattern settings"),
            _ => None,
        };
        if let Some(what) = not_imported
            && !filter_params.is_empty()
        {
            self.warnings.add(format!("graph `{graph_name}`: the {what} of `{}` nodes are not imported yet; they use MaterialCraft's defaults", filter.def));
        }
    }
}

/// Graph outputs as uid → (identifier, usage index).
fn read_graph_outputs(element: XmlNode) -> HashMap<String, (String, u32)> {
    let mut outputs = HashMap::new();
    for output in child_element(element, "graphOutputs").into_iter().flat_map(|o| children_named(o, "graphoutput")) {
        let identifier = child_value(output, "identifier").unwrap_or("output").to_string();
        let usage = child_element(output, "usages")
            .and_then(|u| child_element(u, "usage"))
            .and_then(|u| child_value(u, "name"))
            .map_or(usage_index("any"), usage_index);
        outputs.insert(child_value(output, "uid").unwrap_or("").to_string(), (identifier, usage));
    }
    outputs
}

/// Output size, format and random seed.
fn read_base_params(params: Option<XmlNode>) -> BaseParams {
    let mut base = BaseParams::default();
    for param in params.into_iter().flat_map(|p| children_named(p, "parameter")) {
        let relative_to = child_value(param, "relativeTo").and_then(|r| r.trim().parse::<i32>().ok()).unwrap_or(1);
        let Some(value) = child_element(param, "paramValue").and_then(sbs_constant) else { continue };

        match child_value(param, "name") {
            Some("outputsize") => {
                base.size_mode = match relative_to {
                    0 => SizeMode::Absolute,
                    2 => SizeMode::RelativeToParent,
                    _ => SizeMode::RelativeToInput,
                };
                let limit = |log2: i32| log2.clamp(-(MAX_SIZE_LOG2 as i32), MAX_SIZE_LOG2 as i32);
                base.size = [limit(value.int(0)), limit(value.int(1))];
            }
            Some("format") if relative_to == 0 => {
                base.format = match value.int(0) {
                    0 => OutputFormat::U8,
                    1 => OutputFormat::U16,
                    2 => OutputFormat::F16,
                    3 => OutputFormat::F32,
                    _ => OutputFormat::Inherit,
                }
            }
            Some("randomseed") if relative_to == 0 => base.seed = value.int(0),
            _ => {}
        }
    }
    base
}

/// Connect `wires` in one pass: the last wire into an input wins, and wires that would close a cycle are dropped.
fn connect_all(graph: &mut Graph, wires: Vec<Connection>, warnings: &mut Warnings) {
    let graph_name = graph.name.clone();
    let cycle_warning = || format!("graph `{graph_name}`: a wire that would make a cycle was dropped");

    let mut last_into_input: HashMap<(NodeId, String), usize> = HashMap::new();
    let mut kept: Vec<Option<Connection>> = Vec::with_capacity(wires.len());
    for wire in wires {
        if wire.from == wire.to {
            warnings.add(cycle_warning());
            continue;
        }
        if let Some(previous) = last_into_input.insert((wire.to, wire.to_port.clone()), kept.len())
            && let Some(slot) = kept.get_mut(previous)
        {
            *slot = None;
        }
        kept.push(Some(wire));
    }
    let wires: Vec<Connection> = kept.into_iter().flatten().collect();

    // Kahn's algorithm: nodes it can't order lie on or after a cycle
    let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
    let mut downstream: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for wire in &wires {
        *in_degree.entry(wire.to).or_default() += 1;
        downstream.entry(wire.from).or_default().push(wire.to);
    }
    let mut ready: VecDeque<NodeId> = graph.nodes.iter().map(|n| n.id).filter(|id| !in_degree.contains_key(id)).collect();
    let mut ordered: HashSet<NodeId> = HashSet::new();
    while let Some(id) = ready.pop_front() {
        ordered.insert(id);
        for next in downstream.get(&id).into_iter().flatten() {
            if let Some(degree) = in_degree.get_mut(next) {
                *degree -= 1;
                if *degree == 0 {
                    ready.push_back(*next);
                }
            }
        }
    }

    // Wires among unordered nodes go through the cycle-checked path, one at a time
    let (safe, risky): (Vec<Connection>, Vec<Connection>) = wires.into_iter().partition(|w| ordered.contains(&w.from) || ordered.contains(&w.to));
    graph.connections = safe;
    for wire in risky {
        if graph.connect(wire.from, &wire.from_port, wire.to, &wire.to_port).is_err() {
            warnings.add(cycle_warning());
        }
    }
}

/// Fill in a [`MISSING_NODE`] placeholder with what writing it back needs.
fn fill_placeholder(
    node: &mut Node,
    kind: &str,
    name: &str,
    package: &str,
    raw_params: Option<&str>,
    output_uids: &[(String, bool)],
    bridgings: Option<&[(String, String)]>,
) {
    node.label = name.to_string();
    node.params.insert("sbs_kind".into(), Value::Text(kind.into()));
    node.params.insert("sbs_name".into(), Value::Text(name.into()));
    if !package.is_empty() {
        node.params.insert("sbs_package".into(), Value::Text(package.into()));
    }
    if let Some(raw) = raw_params {
        node.params.insert("sbs_parameters".into(), Value::Text(raw.into()));
    }

    let mut seen = BTreeSet::new();
    let outputs: Vec<String> = placeholder_outputs(output_uids, bridgings)
        .into_iter()
        .filter(|(_, port)| seen.insert(port.clone()))
        .map(|(uid, port)| if output_uids.iter().any(|(u, gray)| *u == uid && *gray) { format!("{port}:gray") } else { port })
        .collect();
    node.params.insert("outputs".into(), Value::Text(outputs.join(",")));
    node.params.insert("inputs".into(), Value::Text(String::new()));
}

/// (Output uid, port) of a placeholder in output order: the instance's bridgings, else `output`, `output_2`…
fn placeholder_outputs(output_uids: &[(String, bool)], bridgings: Option<&[(String, String)]>) -> Vec<(String, String)> {
    let bridged = |uid: &str| bridgings.and_then(|b| b.iter().find(|(u, _)| u == uid)).map(|(_, identifier)| identifier.clone());
    let mut outputs: Vec<(String, String)> = output_uids
        .iter()
        .enumerate()
        .map(|(i, (uid, _))| (uid.clone(), bridged(uid).unwrap_or_else(|| if i == 0 { "output".to_string() } else { format!("output_{}", i + 1) })))
        .collect();

    for (uid, identifier) in bridgings.unwrap_or_default() {
        if !outputs.iter().any(|(u, _)| u == uid) {
            outputs.push((uid.clone(), identifier.clone()));
        }
    }
    outputs
}

/// Record a wire arriving at placeholder input `port`.
fn add_placeholder_input(node: &mut Node, port: &str) {
    if port.is_empty() || port.contains(',') {
        return;
    }

    let (mut inputs, _) = missing_ports(node);
    if !inputs.iter().any(|p| p == port) {
        inputs.push(port.to_string());
        node.params.insert("inputs".into(), Value::Text(inputs.join(",")));
    }
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

/// Write `pkg` as an SBS package (UTF-8 XML) with bitmaps embedded and MaterialCraft's exact copy at the end.
pub fn write_sbs(pkg: &Package) -> Result<Vec<u8>> {
    let mut writer = SbsWriter { last_uid: FIRST_UID, external_packages: BTreeMap::new() };
    let self_uid = writer.next_uid();
    let library_uid = writer.next_uid();

    let mut content = String::from("<content>");
    for graph in &pkg.graphs {
        writer.write_graph(&mut content, pkg, graph, self_uid, library_uid);
    }
    if !pkg.resources.is_empty() {
        let _ = write!(content, "<group><uid v=\"{}\"/><identifier v=\"Resources\"/><content>", writer.next_uid());
        for resource in &pkg.resources {
            let svg = resource.file.to_ascii_lowercase().ends_with(".svg");
            let ext = resource.file.rsplit_once('.').map_or("png", |(_, e)| e);
            // A resource linked to a file next to the package stays a link; one linked from
            // elsewhere (or opened without a folder) is embedded, so the file is whole.
            let relative = resource.link.as_deref().filter(|l| !l.contains(':') && !l.starts_with('/') && !l.starts_with('\\'));
            match relative {
                Some(path) => {
                    let _ = write!(
                        content,
                        "<resource><identifier v=\"{}\"/><uid v=\"{}\"/><type v=\"{}\"/><format v=\"{}\"/><filepath v=\"{}\"/></resource>",
                        xml_escape(&resource.name),
                        writer.next_uid(),
                        if svg { "svg" } else { "bitmap" },
                        xml_escape(ext),
                        xml_escape(path)
                    );
                }
                None => {
                    let _ = write!(
                        content,
                        "<resource><identifier v=\"{}\"/><uid v=\"{}\"/><type v=\"{}\"/><format v=\"{}\"/><filepath v=\"\"/><source><binembedded><datalength v=\"{}\"/><strdata v=\"{}\"/></binembedded></source></resource>",
                        xml_escape(&resource.name),
                        writer.next_uid(),
                        if svg { "svg" } else { "bitmap" },
                        xml_escape(ext),
                        resource.bytes.len(),
                        encode_embedded(resource.bytes.as_slice())?
                    );
                }
            }
        }
        content.push_str("</content></group>");
    }
    content.push_str("</content>");

    let mut out = String::with_capacity(content.len() + 4096);
    let _ = write!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package><identifier v=\"{}\"/><formatVersion v=\"{FORMAT_VERSION}\"/><updaterVersion v=\"{FORMAT_VERSION}\"/><fileUID v=\"0\"/><versionUID v=\"0\"/><dependencies>",
        xml_escape(&pkg.name)
    );
    let write_dependency = |out: &mut String, file: &str, uid: u64| {
        let _ = write!(
            out,
            "<dependency><filename v=\"{}\"/><uid v=\"{uid}\"/><type v=\"package\"/><fileUID v=\"0\"/><versionUID v=\"0\"/></dependency>",
            xml_escape(file)
        );
    };
    write_dependency(&mut out, SELF_DEPENDENCY, self_uid);
    write_dependency(&mut out, LIBRARY_DEPENDENCY, library_uid);
    for (file, uid) in &writer.external_packages {
        write_dependency(&mut out, file, *uid);
    }
    out.push_str("</dependencies>");
    out.push_str(&content);

    // The exact copy; resource bytes live in the resources above
    let json = serde_json::to_vec(pkg).map_err(|e| FormatError::Io(e.to_string()))?;
    let hash = content_hash(content.as_bytes());
    let _ = write!(out, "<!-- {EXACT_MARK} version={EXACT_VERSION} hash={hash:016x} data={} -->", encode_embedded(&json)?);
    out.push_str("</package>\n");
    Ok(out.into_bytes())
}

struct SbsWriter {
    last_uid: u64,
    /// Packages placeholders come from: file name → dependency uid.
    external_packages: BTreeMap<String, u64>,
}

/// An output port's SBS type: 1 color, 2 grayscale.
fn sbs_output_type(kind: PortKind) -> u8 {
    if kind == PortKind::Gray { 2 } else { 1 }
}

/// Text parameter `key` of `node`, or `default` when unset or empty.
fn text_param(node: &Node, key: &str, default: &str) -> String {
    match node.params.get(key) {
        Some(Value::Text(s)) if !s.is_empty() => s.clone(),
        _ => default.to_string(),
    }
}

/// A node's output ports as (port, SBS type, uid).
type WrittenOutputs = Vec<(String, u8, u64)>;

impl SbsWriter {
    fn next_uid(&mut self) -> u64 {
        self.last_uid = self.last_uid.saturating_add(1);
        self.last_uid
    }

    fn external_package_uid(&mut self, file: &str) -> u64 {
        if let Some(uid) = self.external_packages.get(file) {
            return *uid;
        }

        let uid = self.next_uid();
        self.external_packages.insert(file.to_string(), uid);
        uid
    }

    /// The output ports of `node` as (port, SBS type).
    fn output_ports(pkg: &Package, node: &Node) -> Vec<(String, u8)> {
        match node.def.as_str() {
            "output" => Vec::new(),
            "graph_instance" => {
                let sub_graph = match node.params.get("graph") {
                    Some(Value::Text(name)) => pkg.graph_by_name(name).map(|(_, g)| g),
                    _ => None,
                };
                let outputs = sub_graph.into_iter().flat_map(|g| g.nodes.iter().filter(|o| o.def == "output"));
                outputs.map(|o| (text_param(o, "identifier", "output"), 1)).collect()
            }
            def if def == MISSING_NODE => missing_ports(node).1.into_iter().map(|(id, gray)| (id, if gray { 2 } else { 1 })).collect(),
            def => match find(def) {
                Some(def) => def.outputs.iter().map(|o| (o.id.to_string(), sbs_output_type(o.kind))).collect(),
                None => vec![("output".into(), 1)],
            },
        }
    }

    fn write_graph(&mut self, out: &mut String, pkg: &Package, graph: &Graph, self_uid: u64, library_uid: u64) {
        let _ = write!(out, "<graph><identifier v=\"{}\"/><uid v=\"{}\"/><graphtype v=\"material\"/>", xml_escape(&graph.name), self.next_uid());
        let attributes = &graph.attributes;
        if !(attributes.category.is_empty() && attributes.description.is_empty() && attributes.author.is_empty() && attributes.tags.is_empty()) {
            let _ = write!(
                out,
                "<attributes><category v=\"{}\"/><description v=\"{}\"/><author v=\"{}\"/><tags v=\"{}\"/></attributes>",
                xml_escape(&attributes.category),
                xml_escape(&attributes.description),
                xml_escape(&attributes.author),
                xml_escape(&attributes.tags)
            );
        }

        let mut node_uids: HashMap<NodeId, u64> = HashMap::new();
        let mut node_outputs: HashMap<NodeId, WrittenOutputs> = HashMap::new();
        for node in &graph.nodes {
            node_uids.insert(node.id, self.next_uid());
            let outputs = Self::output_ports(pkg, node).into_iter().map(|(port, kind)| (port, kind, self.next_uid())).collect();
            node_outputs.insert(node.id, outputs);
        }

        let input_entries = self.write_graph_inputs(out, graph);
        let output_uids = self.write_graph_outputs(out, graph);

        let mut wires_into: HashMap<NodeId, Vec<&Connection>> = HashMap::new();
        for wire in &graph.connections {
            wires_into.entry(wire.to).or_default().push(wire);
        }
        out.push_str("<compNodes>");
        for node in &graph.nodes {
            let uid = node_uids.get(&node.id).copied().unwrap_or(0);
            let (x, y) = (node.pos[0] + NODE_HALF_SIZE[0], node.pos[1] + NODE_HALF_SIZE[1]);
            let _ = write!(out, "<compNode><uid v=\"{uid}\"/><GUILayout><gpos v=\"{} {} 0\"/></GUILayout>", format_float(x), format_float(y));

            let outputs = node_outputs.get(&node.id).map_or(&[][..], Vec::as_slice);
            if !outputs.is_empty() {
                out.push_str("<compOutputs>");
                for (_, kind, output_uid) in outputs {
                    let _ = write!(out, "<compOutput><uid v=\"{output_uid}\"/><comptype v=\"{kind}\"/></compOutput>");
                }
                out.push_str("</compOutputs>");
            }

            let filter = filter_by_def(&node.def);
            let wires = wires_into.get(&node.id).map_or(&[][..], Vec::as_slice);
            if !wires.is_empty() {
                out.push_str("<connections>");
                for wire in wires {
                    let Some(from_uid) = node_uids.get(&wire.from) else { continue };
                    let Some((_, _, output_uid)) = node_outputs.get(&wire.from).and_then(|o| o.iter().find(|(port, _, _)| *port == wire.from_port)) else {
                        continue;
                    };

                    let identifier = if node.def == "output" {
                        "inputNodeOutput".to_string()
                    } else {
                        let mapped = filter.and_then(|f| f.inputs.iter().find(|(_, port)| *port == wire.to_port));
                        mapped.map_or(wire.to_port.clone(), |(sbs, _)| sbs.to_string())
                    };
                    let _ = write!(
                        out,
                        "<connection><identifier v=\"{}\"/><connRef v=\"{from_uid}\"/><connRefOutput v=\"{output_uid}\"/></connection>",
                        xml_escape(&identifier)
                    );
                }
                out.push_str("</connections>");
            }

            out.push_str("<compImplementation>");
            let ids = NodeUids { input_entry: input_entries.get(&node.id).copied(), graph_output: output_uids.get(&node.id).copied(), self_uid, library_uid };
            self.write_implementation(out, node, outputs, ids);
            out.push_str("</compImplementation></compNode>");
        }
        out.push_str("</compNodes><baseParameters/>");

        write_canvas_items(out, graph, self);
        let [width, height] = graph.size.map(|s| s.clamp(MIN_SIZE_LOG2, MAX_SIZE_LOG2));
        let _ = write!(out, "<options><option><name v=\"defaultParentSize\"/><value v=\"{width}x{height}\"/></option></options><root><rootOutputs>");
        for uid in graph.nodes.iter().filter_map(|n| output_uids.get(&n.id)) {
            let _ = write!(out, "<rootOutput><output v=\"{uid}\"/><format v=\"0\"/><usertag v=\"\"/></rootOutput>");
        }
        out.push_str("</rootOutputs></root></graph>");
    }

    /// Image inputs (input nodes) and exposed values; returns input node → paraminput uid.
    fn write_graph_inputs(&mut self, out: &mut String, graph: &Graph) -> HashMap<NodeId, u64> {
        let mut entries = HashMap::new();
        out.push_str("<paraminputs>");

        for node in graph.nodes.iter().filter(|n| n.def == "input_color" || n.def == "input_grayscale") {
            let uid = self.next_uid();
            entries.insert(node.id, uid);
            let default = match node.params.get("default") {
                Some(color @ Value::Color(_)) => constant_xml(color).unwrap_or_default(),
                _ => "<constantValueFloat4 v=\"0.5 0.5 0.5 1\"/>".into(),
            };
            let code = if node.def == "input_grayscale" { type_code::GRAY_IMAGE } else { type_code::COLOR_IMAGE };
            let _ = write!(
                out,
                "<paraminput><identifier v=\"{}\"/><uid v=\"{uid}\"/><isConnectable v=\"1\"/><type v=\"{code}\"/><defaultValue>{default}</defaultValue><defaultWidget><name v=\"\"/><options/></defaultWidget></paraminput>",
                xml_escape(&text_param(node, "identifier", "input"))
            );
        }

        for exposed in &graph.exposed {
            let default = constant_xml(&exposed.default).unwrap_or_else(|| "<constantValueFloat1 v=\"0\"/>".into());
            let group = if exposed.group.is_empty() { String::new() } else { format!("<group v=\"{}\"/>", xml_escape(&exposed.group)) };
            let _ = write!(
                out,
                "<paraminput><identifier v=\"{}\"/><uid v=\"{}\"/><attributes><label v=\"{}\"/></attributes><type v=\"{}\"/><defaultValue>{default}</defaultValue><defaultWidget><name v=\"slider\"/><options><option><name v=\"min\"/><value v=\"{}\"/></option><option><name v=\"max\"/><value v=\"{}\"/></option></options></defaultWidget>{group}</paraminput>",
                xml_escape(&exposed.id),
                self.next_uid(),
                xml_escape(&exposed.label),
                exposed_type_code(&exposed.default),
                format_float(exposed.min),
                format_float(exposed.max)
            );
        }

        out.push_str("</paraminputs>");
        entries
    }

    /// Graph outputs (output nodes); returns output node → graphoutput uid.
    fn write_graph_outputs(&mut self, out: &mut String, graph: &Graph) -> HashMap<NodeId, u64> {
        let mut output_uids = HashMap::new();
        out.push_str("<graphOutputs>");

        for node in graph.nodes.iter().filter(|n| n.def == "output") {
            let uid = self.next_uid();
            output_uids.insert(node.id, uid);
            let usage = node.params.get("usage").and_then(Value::as_i32).unwrap_or(0).max(0) as u32;
            let identifier = text_param(node, "identifier", "output");
            let label = if node.label.is_empty() { identifier.clone() } else { node.label.clone() };
            let _ = write!(
                out,
                "<graphoutput><identifier v=\"{}\"/><uid v=\"{uid}\"/><attributes><label v=\"{}\"/></attributes><usages><usage><components v=\"RGBA\"/><name v=\"{}\"/></usage></usages><group v=\"Material\"/></graphoutput>",
                xml_escape(&identifier),
                xml_escape(&label),
                usage_name(usage)
            );
        }

        out.push_str("</graphOutputs>");
        output_uids
    }

    /// The `compImplementation` body of `node`.
    fn write_implementation(&mut self, out: &mut String, node: &Node, outputs: &[(String, u8, u64)], ids: NodeUids) {
        let base = base_params_xml(&node.base);

        match node.def.as_str() {
            "output" => {
                let _ = write!(out, "<compOutputBridge><output v=\"{}\"/></compOutputBridge>", ids.graph_output.unwrap_or(0));
            }
            "input_color" | "input_grayscale" => {
                let _ = write!(out, "<compInputBridge><entry v=\"{}\"/><parameters>{base}</parameters></compInputBridge>", ids.input_entry.unwrap_or(0));
            }
            "graph_instance" => {
                let sub_graph = text_param(node, "graph", "");
                let params: String = node.params.iter().filter(|(k, _)| *k != "graph").filter_map(|(k, v)| param_xml(k, v)).collect();
                let _ = write!(
                    out,
                    "<compInstance><path v=\"pkg:///{}?dependency={}\"/><parameters>{base}{params}</parameters>",
                    xml_escape(&sub_graph),
                    ids.self_uid
                );
                write_bridgings(out, outputs);
                out.push_str("</compInstance>");
            }
            def if def == MISSING_NODE => {
                let name = text_param(node, "sbs_name", &node.label);
                let raw_params = match node.params.get("sbs_parameters") {
                    Some(Value::Text(s)) if is_parameters_element(s) => s.clone(),
                    _ => "<parameters/>".into(),
                };
                if text_param(node, "sbs_kind", "instance") == "filter" {
                    let _ = write!(out, "<compFilter><filter v=\"{}\"/>{raw_params}</compFilter>", xml_escape(&name));
                    return;
                }

                let package = text_param(node, "sbs_package", "");
                let dependency = if package.is_empty() { ids.library_uid } else { self.external_package_uid(&package) };
                let path = text_param(node, "sbs_path", &name);
                let _ = write!(out, "<compInstance><path v=\"pkg:///{}?dependency={dependency}\"/>{raw_params}", xml_escape(&path));
                write_bridgings(out, outputs);
                out.push_str("</compInstance>");
            }
            def => match (filter_by_def(def), find(def)) {
                (Some(filter), Some(catalog_def)) => {
                    let _ = write!(out, "<compFilter><filter v=\"{}\"/><parameters>{base}", filter.sbs);
                    let value_of = |id: &str| node.params.get(id).or_else(|| catalog_def.param(id).map(|p| &p.default));
                    for (sbs_name, mc_name, conversion) in filter.params {
                        let Some(value) = value_of(mc_name) else { continue };
                        let value = if *conversion == TrueIsFirstOption { Value::Bool(value.as_i32() == Some(0)) } else { value.clone() };
                        out.push_str(&param_xml(sbs_name, &value).unwrap_or_default());
                    }
                    out.push_str(&special_filter_params_xml(filter.sbs, &value_of, ids.self_uid));
                    out.push_str("</parameters></compFilter>");
                }
                _ => {
                    let params: String = node.params.iter().filter_map(|(k, v)| param_xml(k, v)).collect();
                    let _ = write!(
                        out,
                        "<compInstance><path v=\"pkg:///{}?dependency={}\"/><parameters>{base}{params}</parameters>",
                        xml_escape(def),
                        ids.library_uid
                    );
                    write_bridgings(out, outputs);
                    out.push_str("</compInstance>");
                }
            },
        }
    }
}

/// The uids a node's implementation refers to.
#[derive(Clone, Copy)]
struct NodeUids {
    input_entry: Option<u64>,
    graph_output: Option<u64>,
    self_uid: u64,
    library_uid: u64,
}

/// Filter parameters that don't map one to one: the bitmap path, grayscale weights, the 2×2 matrix.
fn special_filter_params_xml<'a>(filter: &str, value_of: &dyn Fn(&str) -> Option<&'a Value>, self_uid: u64) -> String {
    match filter {
        "bitmap" => match value_of("resource") {
            Some(Value::Text(name)) if !name.is_empty() => {
                param_xml("bitmapresourcepath", &Value::Text(format!("pkg:///Resources/{name}?dependency={self_uid}"))).unwrap_or_default()
            }
            _ => String::new(),
        },
        "grayscaleconversion" => {
            let weight = |channel: &str| value_of(channel).and_then(Value::as_f32).unwrap_or(0.0);
            param_xml("channelsweights", &Value::Float4([weight("red"), weight("green"), weight("blue"), weight("alpha")])).unwrap_or_default()
        }
        "transformation" => {
            let scale = match value_of("scale") {
                Some(Value::Float2(s)) => *s,
                _ => [1.0, 1.0],
            };
            let (sin, cos) = (value_of("rotation").and_then(Value::as_f32).unwrap_or(0.0) * std::f32::consts::TAU).sin_cos();
            param_xml("matrix22", &Value::Float4([scale[0] * cos, -scale[1] * sin, scale[0] * sin, scale[1] * cos])).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Frames (framed comments) and comments.
fn write_canvas_items(out: &mut String, graph: &Graph, writer: &mut SbsWriter) {
    out.push_str("<GUIObjects>");

    for frame in &graph.frames {
        let [x, y, width, height] = frame.rect.map(format_float);
        let [r, g, b] = [0, 1, 2].map(|i| format_float(linear_to_srgb(frame.color[i])));
        let _ = write!(
            out,
            "<GUIObject><type v=\"COMMENT\"/><GUILayout><gpos v=\"{x} {y} -100\"/><size v=\"{width} {height}\"/></GUILayout><GUIName v=\"{}\"/><uid v=\"{}\"/><title v=\"{}\"/><frameColor v=\"{r} {g} {b} {}\"/><isTitleVisible v=\"1\"/><isFrameVisible v=\"1\"/></GUIObject>",
            xml_escape(&frame.description),
            writer.next_uid(),
            xml_escape(&frame.title),
            format_float(frame.color[3])
        );
    }

    for comment in &graph.comments {
        let [x, y] = comment.pos.map(format_float);
        let [width, height] = comment.size.map(format_float);
        let _ = write!(
            out,
            "<GUIObject><type v=\"COMMENT\"/><GUILayout><gpos v=\"{x} {y} 0\"/><size v=\"{width} {height}\"/></GUILayout><GUIName v=\"{}\"/><uid v=\"{}\"/><isFrameVisible v=\"0\"/></GUIObject>",
            xml_escape(&comment.text),
            writer.next_uid()
        );
    }

    out.push_str("</GUIObjects>");
}

fn write_bridgings(out: &mut String, outputs: &[(String, u8, u64)]) {
    out.push_str("<outputBridgings>");
    for (port, _, uid) in outputs {
        let _ = write!(out, "<outputBridging><uid v=\"{uid}\"/><identifier v=\"{}\"/></outputBridging>", xml_escape(port));
    }
    out.push_str("</outputBridgings>");
}

/// One `<parameter>` element (`None` for values SBS constants can't hold).
fn param_xml(name: &str, value: &Value) -> Option<String> {
    Some(format!("<parameter><name v=\"{}\"/><relativeTo v=\"0\"/><paramValue>{}</paramValue></parameter>", xml_escape(name), constant_xml(value)?))
}

fn base_params_xml(base: &BaseParams) -> String {
    let mut out = String::new();

    if base.size_mode != SizeMode::RelativeToInput || base.size != [0, 0] {
        let relative_to = match base.size_mode {
            SizeMode::Absolute => 0,
            SizeMode::RelativeToInput => 1,
            SizeMode::RelativeToParent => 2,
        };
        let [width, height] = base.size;
        let _ = write!(
            out,
            "<parameter><name v=\"outputsize\"/><relativeTo v=\"{relative_to}\"/><paramValue><constantValueInt2 v=\"{width} {height}\"/></paramValue></parameter>"
        );
    }

    let format = match base.format {
        OutputFormat::Inherit => None,
        OutputFormat::U8 => Some(0),
        OutputFormat::U16 => Some(1),
        OutputFormat::F16 => Some(2),
        OutputFormat::F32 => Some(3),
    };
    if let Some(format) = format {
        let _ = write!(out, "<parameter><name v=\"format\"/><relativeTo v=\"0\"/><paramValue><constantValueInt1 v=\"{format}\"/></paramValue></parameter>");
    }

    if base.seed != 0 {
        let _ = write!(
            out,
            "<parameter><name v=\"randomseed\"/><relativeTo v=\"0\"/><paramValue><constantValueInt1 v=\"{}\"/></paramValue></parameter>",
            base.seed
        );
    }
    out
}

/// Whether `xml` is one well-formed `<parameters>` element, the raw SBS a placeholder keeps.
fn is_parameters_element(xml: &str) -> bool {
    let options = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 1_000_000 };
    xml.len() < (16 << 20) && roxmltree::Document::parse_with_options(xml, options).is_ok_and(|d| d.root_element().tag_name().name() == "parameters")
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_core::Graph;

    fn no_linked_files(_: &str) -> Option<Vec<u8>> {
        None
    }

    /// The SBS data of a written package without the exact copy, as another program would leave it.
    fn without_exact_copy(bytes: &[u8]) -> String {
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        format!("{}</package>", &text[..text.find("<!--").unwrap()])
    }

    fn sample_package() -> Package {
        let mut pkg = Package::new("Bricks");
        let mut graph = Graph::new("Wall");
        graph.size = [9, 9];

        let bricks = graph.add_node("brick_generator", [0.0, 0.0]);
        let blur = graph.add_node("blur", [150.0, 0.0]);
        let uniform = graph.add_node("uniform", [0.0, 150.0]);
        let blend = graph.add_node("blend", [300.0, 0.0]);
        let output = graph.add_node("output", [450.0, 0.0]);
        graph.connect(bricks, "output", blur, "input").unwrap();
        graph.connect(blur, "output", blend, "foreground").unwrap();
        graph.connect(uniform, "output", blend, "background").unwrap();
        graph.connect(blend, "output", output, "input").unwrap();

        let blur_node = graph.node_mut(blur).unwrap();
        blur_node.params.insert("intensity".into(), Value::Float(4.5));
        blur_node.base.size_mode = SizeMode::Absolute;
        blur_node.base.size = [8, 8];
        graph.node_mut(uniform).unwrap().params.insert("color".into(), Value::Color([0.25, 0.5, 0.75, 1.0]));
        graph.node_mut(blend).unwrap().params.insert("mode".into(), Value::Enum(3));
        let output_node = graph.node_mut(output).unwrap();
        output_node.params.insert("usage".into(), Value::Enum(4));
        output_node.params.insert("identifier".into(), Value::Text("height".into()));
        // Only the exact copy can carry a curve
        graph.node_mut(bricks).unwrap().params.insert("curvy".into(), Value::Curve(materialcraft_core::Curve::default()));
        graph.frames.push(Frame {
            id: 90,
            title: "Frame & <title>".into(),
            description: "note".into(),
            rect: [-10.0, -10.0, 300.0, 200.0],
            color: [0.1, 0.2, 0.3, 0.4],
        });
        pkg.graphs.push(graph);

        let orange = materialcraft_image::Image::filled(4, 4, materialcraft_image::Kind::Color, [1.0, 0.5, 0.0, 1.0]);
        let png = crate::encode(&orange, "png", crate::Depth::U8, true).unwrap();
        pkg.resources.push(Resource::new("tex", "tex.png", ResourceBytes::new(png)));
        pkg
    }

    #[test]
    fn sniffs_sbs() {
        assert!(is_sbs(b"<?xml version=\"1.0\"?>\n<package><content/></package>"));
        assert!(is_sbs(b"\xef\xbb\xbf<package>"));
        assert!(is_sbs(b"<!-- hi --><package >"));
        assert!(!is_sbs(b"<packages>"));
        assert!(!is_sbs(b"PK\x03\x04zip"));
        assert!(!is_sbs(b"<?xml"));
        assert!(!is_sbs(b""));
    }

    #[test]
    fn exact_round_trip() {
        let pkg = sample_package();
        let bytes = write_sbs(&pkg).unwrap();
        assert!(is_sbs(&bytes));

        let read = read_sbs(&bytes, "Bricks.sbs", &no_linked_files).unwrap();
        assert!(read.exact, "{:?}", read.warnings);
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert_eq!(read.pkg.graphs, pkg.graphs);
        assert_eq!(read.pkg.resources[0].bytes, pkg.resources[0].bytes);
        assert_eq!(read.pkg.name, "Bricks");
    }

    /// The SBS data alone carries the graph.
    #[test]
    fn structural_round_trip() {
        let pkg = sample_package();
        let plain = without_exact_copy(&write_sbs(&pkg).unwrap());
        let read = read_sbs(plain.as_bytes(), "x.sbs", &no_linked_files).unwrap();
        assert!(!read.exact);

        let (original, back) = (&pkg.graphs[0], &read.pkg.graphs[0]);
        assert_eq!(back.name, "Wall");
        assert_eq!(back.size, [9, 9]);
        assert_eq!(back.nodes.len(), original.nodes.len());
        assert_eq!(back.connections.len(), original.connections.len());
        for (a, b) in original.nodes.iter().zip(&back.nodes) {
            assert_eq!(a.def, b.def);
            assert!((a.pos[0] - b.pos[0]).abs() < 1e-3 && (a.pos[1] - b.pos[1]).abs() < 1e-3);
            assert_eq!(a.base, b.base);
        }

        assert_eq!(back.nodes[1].params.get("intensity"), Some(&Value::Float(4.5)));
        let Some(Value::Color(color)) = back.nodes[2].params.get("color") else { panic!("uniform color") };
        assert!((color[0] - 0.25).abs() < 1e-3 && (color[2] - 0.75).abs() < 1e-3);
        assert_eq!(back.nodes[3].params.get("mode"), Some(&Value::Enum(3)));
        assert_eq!(back.nodes[4].params.get("usage"), Some(&Value::Enum(4)));
        assert_eq!(back.nodes[4].params.get("identifier"), Some(&Value::Text("height".into())));
        assert_eq!(back.frames[0].title, "Frame & <title>");
        assert_eq!(read.pkg.resources[0].bytes, pkg.resources[0].bytes);

        let blend = back.nodes[3].id;
        assert_eq!(back.input_of(blend, "foreground").map(|c| c.from), Some(back.nodes[1].id));
        assert_eq!(back.input_of(blend, "background").map(|c| c.from), Some(back.nodes[2].id));
    }

    #[test]
    fn edited_file_falls_back_to_sbs_data() {
        let text = String::from_utf8(write_sbs(&sample_package()).unwrap()).unwrap().replace("<identifier v=\"Wall\"/>", "<identifier v=\"Wall2\"/>");
        let read = read_sbs(text.as_bytes(), "x.sbs", &no_linked_files).unwrap();

        assert!(!read.exact);
        assert_eq!(read.pkg.graphs[0].name, "Wall2");
        assert!(read.warnings.iter().any(|w| w.contains("changed outside")), "{:?}", read.warnings);
    }

    #[test]
    fn damaged_exact_copy_falls_back_with_a_warning() {
        let text = String::from_utf8(write_sbs(&sample_package()).unwrap()).unwrap();
        let data_start = text.find("data=").unwrap() + "data=".len();
        let data_end = data_start + text[data_start..].find(' ').unwrap();
        let damaged = format!("{}AAAAAAAAAAAA{}", &text[..data_start], &text[data_end..]);
        let read = read_sbs(damaged.as_bytes(), "x.sbs", &no_linked_files).unwrap();

        assert!(!read.exact);
        assert_eq!(read.pkg.graphs[0].nodes.len(), 5);
        assert!(read.warnings.iter().any(|w| w.contains("damaged")), "{:?}", read.warnings);
    }

    #[test]
    fn missing_nodes_are_placeholders_that_survive_saving() {
        let xml = r#"<?xml version="1.0"?><package><identifier v="P"/><dependencies><dependency><filename v="sbs://noise_fancy.sbs"/><uid v="7"/></dependency></dependencies><content><graph><identifier v="G"/>
            <compNodes>
              <compNode><uid v="1"/><GUILayout><gpos v="0 0 0"/></GUILayout><compOutputs><compOutput><uid v="11"/><comptype v="2"/></compOutput></compOutputs>
                <compImplementation><compInstance><path v="pkg:///fancy_noise?dependency=7"/><parameters><parameter><name v="scale"/><paramValue><constantValueFloat1 v="3"/></paramValue></parameter></parameters>
                <outputBridgings><outputBridging><uid v="11"/><identifier v="noise"/></outputBridging></outputBridgings></compInstance></compImplementation></compNode>
              <compNode><uid v="2"/><GUILayout><gpos v="200 0 0"/></GUILayout><compOutputs><compOutput><uid v="21"/><comptype v="1"/></compOutput></compOutputs>
                <connections><connection><identifier v="input1"/><connRef v="1"/><connRefOutput v="11"/></connection></connections>
                <compImplementation><compFilter><filter v="blur"/><parameters><parameter><name v="intensity"/><paramValue><constantValueFloat1 v="6"/></paramValue></parameter><parameter><name v="bogus"/><paramValue><constantValueFloat1 v="6"/></paramValue></parameter></parameters></compFilter></compImplementation></compNode>
              <compNode><uid v="3"/><GUILayout><gpos v="400 0 0"/></GUILayout><compOutputs><compOutput><uid v="31"/><comptype v="1"/></compOutput></compOutputs>
                <connections><connection><identifier v="weird_in"/><connRef v="2"/><connRefOutput v="21"/></connection></connections>
                <compImplementation><compFilter><filter v="futurefilter"/><parameters/></compFilter></compImplementation></compNode>
            </compNodes></graph></content></package>"#;
        let read = read_sbs(xml.as_bytes(), "p.sbs", &no_linked_files).unwrap();
        let graph = &read.pkg.graphs[0];
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.connections.len(), 2);

        let fancy = &graph.nodes[0];
        assert_eq!(fancy.def, MISSING_NODE);
        assert_eq!(fancy.label, "fancy_noise");
        assert_eq!(missing_ports(fancy).1, vec![("noise".to_string(), true)]);
        assert_eq!(graph.nodes[1].params.get("intensity"), Some(&Value::Float(6.0)));
        assert_eq!(graph.input_of(graph.nodes[1].id, "input").map(|c| c.from_port.as_str()), Some("noise"));
        assert_eq!(missing_ports(&graph.nodes[2]).0, vec!["weird_in".to_string()]);
        for expected in ["missing node `fancy_noise`", "missing node `futurefilter`", "bogus"] {
            assert!(read.warnings.iter().any(|w| w.contains(expected)), "{expected}: {:?}", read.warnings);
        }

        // Saved and read back from the SBS data alone, the placeholders are written as they came
        let text = String::from_utf8(write_sbs(&read.pkg).unwrap()).unwrap();
        assert!(text.contains("sbs://noise_fancy.sbs"));
        assert!(text.contains("pkg:///fancy_noise?dependency="));
        assert!(text.contains("<filter v=\"futurefilter\"/>"));
        let back = read_sbs(without_exact_copy(text.as_bytes()).as_bytes(), "p.sbs", &no_linked_files).unwrap();
        let back_graph = &back.pkg.graphs[0];
        assert_eq!(back_graph.nodes[0].def, MISSING_NODE);
        assert_eq!(back_graph.nodes[0].params.get("sbs_parameters"), fancy.params.get("sbs_parameters"));
        assert_eq!(back_graph.connections.len(), 2);
    }

    /// Wires are connected in one pass: the last wire into an input wins and a cycle loses one wire.
    #[test]
    fn wires_keep_the_last_per_input_and_drop_cycles() {
        let node = |uid: u32, wires: &str| {
            format!(
                "<compNode><uid v=\"{uid}\"/><compOutputs><compOutput><uid v=\"{}\"/></compOutput></compOutputs><connections>{wires}</connections><compImplementation><compFilter><filter v=\"blend\"/></compFilter></compImplementation></compNode>",
                uid + 100
            )
        };
        let wire = |input: &str, from: u32| {
            format!("<connection><identifier v=\"{input}\"/><connRef v=\"{from}\"/><connRefOutput v=\"{}\"/></connection>", from + 100)
        };
        let xml = format!(
            "<package><content><graph><identifier v=\"G\"/><compNodes>{}{}{}{}</compNodes></graph></content></package>",
            node(1, ""),
            node(2, &format!("{}{}", wire("source", 1), wire("source", 4))),
            node(3, &wire("source", 2)),
            node(4, &wire("source", 3))
        );
        let read = read_sbs(xml.as_bytes(), "x.sbs", &no_linked_files).unwrap();
        let graph = &read.pkg.graphs[0];

        // 2 takes 4 (the last wire), which closes 2 → 3 → 4 → 2, so one wire of that loop is dropped
        assert_eq!(graph.connections.len(), 2);
        assert!(graph.input_of(graph.nodes[1].id, "foreground").is_none_or(|c| c.from == graph.nodes[3].id));
        assert_eq!(graph.topo_order().len(), 4);
        assert!(read.warnings.iter().any(|w| w.contains("cycle")), "{:?}", read.warnings);
    }

    #[test]
    fn hostile_input_is_an_error_or_a_warning_never_a_panic() {
        let cases: &[&str] = &[
            "",
            "<package",
            "<notpackage/>",
            "<!DOCTYPE package [<!ENTITY x \"y\">]><package/>",
            "<package><content><graph/></content></package>",
            "<package><content><graph><compNodes><compNode/><compNode><uid v=\"1\"/><compImplementation><compInstance/></compImplementation></compNode><compNode><uid v=\"1\"/></compNode></compNodes></graph></content></package>",
            "<package><content><graph><options><option><name v=\"defaultParentSize\"/><value v=\"99999x-4\"/></option></options><compNodes><compNode><uid v=\"1\"/><connections><connection><identifier v=\"input1\"/><connRef v=\"1\"/><connRefOutput v=\"9\"/></connection></connections><compImplementation><compFilter><filter v=\"blur\"/><parameters><parameter><name v=\"outputsize\"/><relativeTo v=\"0\"/><paramValue><constantValueInt2 v=\"99999999999 nan\"/></paramValue></parameter><parameter><name v=\"intensity\"/><paramValue><constantValueFloat1 v=\"inf\"/></paramValue></parameter></parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>",
            "<package><content><resource><identifier v=\"r\"/><source><binembedded><strdata v=\"////\"/></binembedded></source></resource><resource><identifier v=\"s\"/><source><binembedded><strdata v=\"f////w==\"/></binembedded></source></resource><resource><identifier v=\"t\"/><filepath v=\"../../etc/passwd\"/></resource></content></package>",
            "<package><content/><!-- materialcraft-package version=1 hash=0 data=AAAA --></package>",
            "<package><content><group><content><group><content><graph><identifier v=\"Deep\"/></graph></content></group></content></group><function><identifier v=\"f\"/></function><weird/></content></package>",
        ];
        for case in cases {
            if let Ok(read) = read_sbs(case.as_bytes(), "x.sbs", &no_linked_files) {
                assert!(!read.pkg.graphs.is_empty(), "{case}");
            }
        }
        assert!(read_sbs(b"\xff\xfe", "x.sbs", &no_linked_files).is_err());

        let nested = read_sbs(cases[9].as_bytes(), "x.sbs", &no_linked_files).unwrap();
        assert_eq!(nested.pkg.graphs[0].name, "Deep");
        let clamped = read_sbs(cases[6].as_bytes(), "x.sbs", &no_linked_files).unwrap();
        assert_eq!(clamped.pkg.graphs[0].size, [MAX_SIZE_LOG2, MIN_SIZE_LOG2]);
        assert!(clamped.pkg.graphs[0].connections.is_empty(), "a self-wire is a cycle");
    }

    #[test]
    fn linked_resources_load_through_the_callback() {
        let xml = "<package><content><resource><identifier v=\"img\"/><type v=\"bitmap\"/><format v=\"png\"/><source><externalcopy><filename v=\"p.resources/img.png\"/></externalcopy></source></resource></content></package>";

        let found = read_sbs(xml.as_bytes(), "p.sbs", &|path| (path == "p.resources/img.png").then(|| vec![1, 2, 3])).unwrap();
        assert_eq!(found.pkg.resources[0].bytes.as_slice(), &[1, 2, 3]);

        let not_found = read_sbs(xml.as_bytes(), "p.sbs", &no_linked_files).unwrap();
        assert!(not_found.pkg.resources.is_empty());
        assert!(not_found.warnings.iter().any(|w| w.contains("img")), "{:?}", not_found.warnings);
    }

    #[test]
    fn embedded_data_round_trips_and_checks_its_length() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
        let encoded = encode_embedded(&data).unwrap();
        assert_eq!(decode_embedded(&encoded).unwrap(), data);

        let mut raw = base64_engine().decode(&encoded).unwrap();
        raw[3] ^= 1;
        assert!(decode_embedded(&base64_engine().encode(raw)).unwrap_err().contains("declared"));
    }

    /// The sample with every node of the reference library loads softly and round-trips exactly.
    #[test]
    fn the_all_nodes_sample_loads_with_warnings() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sbs");
        let bytes = std::fs::read(dir.join("all_nodes.sbs")).unwrap();
        assert!(is_sbs(&bytes));

        let read = read_sbs(&bytes, "all_nodes.sbs", &|path| std::fs::read(dir.join(path)).ok()).unwrap();
        let graph = &read.pkg.graphs[0];
        let placeholders = graph.nodes.iter().filter(|n| n.def == MISSING_NODE).count();
        assert_eq!(graph.nodes.len(), 536);
        assert!(graph.nodes.len() - placeholders >= 100, "{placeholders} placeholders");
        assert!(placeholders > 300, "{placeholders} placeholders");
        assert!(read.warnings.iter().any(|w| w.contains("missing node `3D_perlin_noise`")), "{:?}", &read.warnings[..5]);
        assert!(read.pkg.resources.iter().any(|r| r.name == "Untitled_Bitmap" && !r.bytes.is_empty()));
        let svg = read.pkg.resources.iter().find(|r| r.name == "Untitled_Vector_Graphics").unwrap();
        assert!(String::from_utf8_lossy(svg.bytes.as_slice()).contains("<svg"));
        assert_eq!(graph.frames.len(), 26);
        assert_eq!(graph.size, [11, 11]);

        let back = read_sbs(&write_sbs(&read.pkg).unwrap(), "x.sbs", &no_linked_files).unwrap();
        assert!(back.exact);
        assert_eq!(back.pkg.graphs, read.pkg.graphs);
    }
}
