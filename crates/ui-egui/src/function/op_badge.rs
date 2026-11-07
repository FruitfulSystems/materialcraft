//! A short text icon for each function operation: what the node is at a glance ("F2", "+",
//! "≥", "sin", "if"), with a colour per category

use egui::Color32;

/// The text icon of a function operation (its id), always from the glyphs the UI font has
pub fn text(id: &str) -> &'static str {
    match id {
        "const_bool" => "B",
        "const_float1" => "F1",
        "const_float2" => "F2",
        "const_float3" => "F3",
        "const_float4" => "F4",
        "const_int1" => "I1",
        "const_int2" => "I2",
        "const_int3" => "I3",
        "const_int4" => "I4",
        "const_string" => "S",
        "get_bool" => "$B",
        "get_float1" => "$F1",
        "get_float2" => "$F2",
        "get_float3" => "$F3",
        "get_float4" => "$F4",
        "get_integer1" => "$I1",
        "get_integer2" => "$I2",
        "get_integer3" => "$I3",
        "get_integer4" => "$I4",
        "get_string" => "$S",
        "set" => "$=",
        "isdefined" => "$?",
        "add" => "+",
        "sub" => "−",
        "mul" => "×",
        "div" => "÷",
        "mod" => "%",
        "neg" => "−x",
        "mulscalar" => "×s",
        "dot" => "a·b",
        "and" => "&&",
        "or" => "||",
        "not" => "¬",
        "eq" => "=",
        "noteq" => "≠",
        "gt" => ">",
        "gteq" => "≥",
        "lr" => "<",
        "lreq" => "≤",
        "abs" => "|x|",
        "floor" => "flr",
        "ceil" => "ceil",
        "sqrt" => "√",
        "exp" => "e^x",
        "log" => "ln",
        "log2" => "log2",
        "pow2" => "2^x",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "atan2" => "atan",
        "rand" => "rnd",
        "min" => "min",
        "max" => "max",
        "pow" => "x^y",
        "lerp" => "lerp",
        "cartesian" => "polar",
        "vector2" => "F2( )",
        "vector3" => "F3( )",
        "vector4" => "F4( )",
        "swizzle1" => ".x",
        "swizzle2" => ".xy",
        "swizzle3" => ".xyz",
        "swizzle4" => ".xyzw",
        "ivector2" => "I2( )",
        "ivector3" => "I3( )",
        "ivector4" => "I4( )",
        "iswizzle1" => "i.x",
        "iswizzle2" => "i.xy",
        "iswizzle3" => "i.xyz",
        "iswizzle4" => "i.xyzw",
        "tofloat" => "(F1)",
        "toint1" => "(I1)",
        "tofloat2" => "(F2)",
        "toint2" => "(I2)",
        "tofloat3" => "(F3)",
        "toint3" => "(I3)",
        "tofloat4" => "(F4)",
        "toint4" => "(I4)",
        "samplecol" => "RGBA",
        "samplelum" => "Gray",
        "ifelse" => "if",
        "sequence" => "seq",
        "while" => "loop",
        "passthrough" => "•",
        _ => "ƒ",
    }
}

/// The colour of a category's badges: the node header colour, lightened so text in it reads
/// on both themes
pub fn color(category: &str) -> Color32 {
    let base = super::geometry::category_color(category);
    let lift = |c: u8| (c as f32 + (255.0 - c as f32) * 0.45) as u8;
    Color32::from_rgb(lift(base.r()), lift(base.g()), lift(base.b()))
}

/// The category's icon (phosphor glyph)
pub fn icon(category: &str) -> &'static str {
    use egui_phosphor::regular as icon;
    match category {
        "Constant" => icon::HASH,
        "Variables" => icon::BRACKETS_CURLY,
        "Vector" => icon::ARROWS_OUT_CARDINAL,
        "Cast" => icon::ARROWS_LEFT_RIGHT,
        "Operator" => icon::PLUS_MINUS,
        "Logical" => icon::CIRCLES_THREE,
        "Comparison" => icon::EQUALS,
        "Function" => icon::FUNCTION,
        "Control" => icon::GIT_FORK,
        "Samplers" => icon::EYEDROPPER_SAMPLE,
        _ => icon::DOTS_THREE,
    }
}

/// One line on what the operation does (the row's tooltip)
pub fn describe(id: &str) -> &'static str {
    match id {
        "const_bool" | "const_float1" | "const_float2" | "const_float3" | "const_float4" | "const_int1" | "const_int2" | "const_int3" | "const_int4"
        | "const_string" => "A constant value of this type, set in the node's properties.",
        "get_bool" | "get_float1" | "get_float2" | "get_float3" | "get_float4" | "get_integer1" | "get_integer2" | "get_integer3" | "get_integer4"
        | "get_string" => "Reads a variable of this type: a graph input, an exposed parameter, or $pos / $size / $time.",
        "set" => "Stores the value in a named variable for later Get nodes in the same function.",
        "isdefined" => "True when the named variable has a value.",
        "add" => "a + b (same types).",
        "sub" => "a − b.",
        "mul" => "a × b, component by component.",
        "div" => "a ÷ b.",
        "mod" => "The remainder of a ÷ b.",
        "neg" => "−a.",
        "mulscalar" => "Every component of a multiplied by the scalar.",
        "dot" => "The dot product of two vectors.",
        "and" | "or" | "not" => "Boolean logic.",
        "eq" | "noteq" | "gt" | "gteq" | "lr" | "lreq" => "Compares a with b; the result is a boolean.",
        "abs" => "|a|.",
        "floor" => "Rounds down to a whole number.",
        "ceil" => "Rounds up to a whole number.",
        "sqrt" => "The square root.",
        "exp" => "e to the power a.",
        "log" => "The natural logarithm.",
        "log2" => "The logarithm base 2.",
        "pow2" => "2 to the power a.",
        "sin" | "cos" | "tan" => "Trigonometry; the input is in radians.",
        "atan2" => "The angle of the vector (y, x), in radians.",
        "rand" => "A random number from 0 to a.",
        "min" => "The smaller of a and b.",
        "max" => "The larger of a and b.",
        "pow" => "a to the power b.",
        "lerp" => "a blended to b by x (0 = a, 1 = b).",
        "cartesian" => "Polar (rho, theta) to cartesian (x, y).",
        "vector2" | "vector3" | "vector4" | "ivector2" | "ivector3" | "ivector4" => "Builds a vector from its components (the first ones, then the last).",
        "swizzle1" | "swizzle2" | "swizzle3" | "swizzle4" | "iswizzle1" | "iswizzle2" | "iswizzle3" | "iswizzle4" => {
            "Picks and reorders components of a vector (the pattern is in the node's properties)."
        }
        "tofloat" | "tofloat2" | "tofloat3" | "tofloat4" => "Converts the value to floats.",
        "toint1" | "toint2" | "toint3" | "toint4" => "Converts the value to integers (truncated).",
        "samplecol" => "The colour of an image input at a position (pixel processors).",
        "samplelum" => "The grayscale value of an image input at a position (pixel processors).",
        "ifelse" => "The if path when the condition is true, else the else path.",
        "sequence" => "Evaluates the first input (for its side effects), returns the last.",
        "while" => "Runs the loop input while the condition holds, after init.",
        "passthrough" => "A dot: passes its input through, to tidy wires.",
        _ => "A function node.",
    }
}
