use super::*;
use cobol_hir::{HirDataName, HirDataRef};

pub(crate) fn emit_expr(expr: &HirExpr) -> String {
    with_active_context(|ctx| emit_expr_with_ctx(expr, ctx))
}

pub(crate) fn display_numeric_ptr(expr: &str) -> String {
    if is_pointer_like_c_expr(expr) {
        format!("(uint8_t*)({expr})")
    } else {
        format!("(uint8_t*)&({expr})")
    }
}

pub(crate) fn display_numeric_const_ptr(expr: &str) -> String {
    if is_pointer_like_c_expr(expr) {
        format!("(const uint8_t*)({expr})")
    } else {
        format!("(const uint8_t*)&({expr})")
    }
}

fn is_pointer_like_c_expr(expr: &str) -> bool {
    let trimmed = expr.trim_start();
    (trimmed.starts_with('(') && trimmed.contains(" + ("))
        || (trimmed.starts_with("((") && trimmed.contains("*)&"))
}

fn alphanumeric_operand_ptr_expr(
    expr: &HirExpr,
    c_expr: &str,
    data_items: &[HirDataItem],
) -> String {
    let is_pointer_like_expr = matches!(expr, HirExpr::ReferenceModification { .. })
        || matches!(expr, HirExpr::DataRef(data_ref) if data_ref.refmod.is_some())
        || c_expr.starts_with('(');
    let is_renames_macro = find_data_item_by_c_name(c_expr, data_items)
        .or_else(|| {
            find_original_data_item_by_sanitized_name(extract_leaf_member(c_expr), data_items)
        })
        .is_some_and(|item| item.renames.is_some() || item.redefines.is_some());
    if is_group_expr(expr, data_items) {
        return format!("(const uint8_t*)&({c_expr})");
    }
    if let Some(item) =
        expr_data_name(expr).and_then(|name| find_data_item_by_name(name, data_items))
    {
        return match item.data_type {
            HirType::Numeric { .. } => display_numeric_const_ptr(c_expr),
            _ => {
                let is_qualified =
                    expr_data_name(expr).is_some_and(|name| !name.qualifiers.is_empty());
                if item.renames.is_some()
                    || item.redefines.is_some()
                    || is_renames_macro
                    || is_pointer_like_expr
                {
                    format!("(const uint8_t*){c_expr}")
                } else if is_qualified || matches!(expr, HirExpr::Subscript { .. }) {
                    format!("(const uint8_t*)&({c_expr})")
                } else {
                    format!("(const uint8_t*){c_expr}")
                }
            }
        };
    }
    format!("(const uint8_t*){c_expr}")
}

pub(crate) fn data_name_to_c_name(name: &HirDataName) -> String {
    if name.qualifiers.is_empty() {
        sanitize_name(name.as_str())
    } else {
        let mut parts: Vec<String> = name
            .qualifiers_outer_to_inner()
            .map(sanitize_name)
            .collect();
        parts.push(sanitize_name(name.as_str()));
        parts.join("__")
    }
}

pub(crate) fn data_ref_base_c_name(data_ref: &HirDataRef) -> String {
    if data_ref.subscripts.is_empty() {
        data_name_to_c_name(&data_ref.name)
    } else {
        emit_subscript_access(&data_ref.name, &data_ref.subscripts)
    }
}

pub(crate) fn emit_data_ref_expr(data_ref: &HirDataRef) -> String {
    let base = data_ref_base_c_name(data_ref);
    if let Some(refmod) = &data_ref.refmod {
        let c_start = emit_expr_as_numeric(&refmod.start);
        format!("({base} + ({c_start} - 1))")
    } else {
        base
    }
}

pub(crate) fn find_data_item_by_name<'a>(
    name: &HirDataName,
    data_items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    if name.qualifiers.is_empty() {
        return find_data_item(name.as_str(), data_items)
            .or_else(|| find_data_item_by_c_name(&data_name_to_c_name(name), data_items));
    }

    let mut current_items = data_items;
    for qualifier in name.qualifiers_outer_to_inner() {
        let qualifier_item = current_items.iter().find(|item| {
            item.name.as_str() == qualifier.as_str()
                || sanitize_name(&item.name) == sanitize_name(qualifier)
        })?;
        match &qualifier_item.data_type {
            HirType::Group { members, .. } => current_items = members,
            _ => {
                return find_data_item_by_c_name(&data_name_to_c_name(name), data_items);
            }
        }
    }

    find_data_item(name.as_str(), current_items)
        .or_else(|| find_data_item_by_c_name(&data_name_to_c_name(name), data_items))
}

pub(crate) fn emit_expr_with_ctx(expr: &HirExpr, ctx: &CodegenContext) -> String {
    let emit_expr = |expr| super::emit_expr_with_ctx(expr, ctx);
    let emit_expr_as_numeric = |expr| super::emit_expr_as_numeric_with_ctx(expr, ctx);
    let emit_expr_as_double = |expr| super::emit_expr_as_double_with_ctx(expr, ctx);
    match expr {
        HirExpr::Literal(HirLiteral::Integer(n)) => format!("((int64_t){n})"),
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            // Validate that the decimal literal contains only safe characters
            // to prevent injection into the generated C source.
            if d.chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
            {
                if !d.contains('.') {
                    let (scaled, _) = parse_decimal_literal(d);
                    scaled.to_string()
                } else {
                    d.to_string()
                }
            } else {
                "0 /* invalid decimal */".to_string()
            }
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            // Return string literal as a C string (used as pointer in intrinsic
            // function arguments such as FUNCTION LOWER-CASE("text"))
            let escaped = escape_c_string(s);
            format!("\"{}\"", escaped)
        }
        HirExpr::Literal(HirLiteral::Zero) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::Space) => "((int64_t)32)".to_string(),
        HirExpr::Literal(HirLiteral::HighValue) => "((int64_t)0xFF)".to_string(),
        HirExpr::Literal(HirLiteral::LowValue) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::Quote) => "((int64_t)'\"')".to_string(),
        HirExpr::Literal(HirLiteral::Null) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            if let Some(ch) = s.chars().next() {
                format!("((int64_t)'{}')", ch)
            } else {
                "((int64_t)' ')".to_string()
            }
        }
        HirExpr::DataRef(data_ref) => emit_data_ref_expr(data_ref),
        HirExpr::Variable(name) => data_name_to_c_name(name),
        HirExpr::BinaryOp { op, left, right } => {
            // Use emit_expr_as_numeric to auto-convert CobolDecimal sub-expressions
            let l = emit_expr_as_numeric(left);
            let r = emit_expr_as_numeric(right);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("((int64_t)pow((double){l}, (double){r}))"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_expr_as_numeric(operand);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_name = name.to_uppercase();
            let c_args: Vec<_> = args.iter().map(emit_expr_as_numeric).collect();
            // Map COBOL intrinsic function names to runtime function calls.
            match upper_name.as_str() {
                "LENGTH" => {
                    // FUNCTION LENGTH(var) -- returns the byte length.
                    if let Some(arg_expr) = args.first() {
                        // Check if the arg is a string-returning function
                        if let HirExpr::FunctionCall {
                            name: inner_name,
                            args: inner_args,
                        } = arg_expr
                        {
                            let inner_upper = inner_name.to_uppercase();
                            match inner_upper.as_str() {
                                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                                    if let Some(inner_arg) = inner_args.first() {
                                        let size = if let HirExpr::Literal(HirLiteral::String(s)) =
                                            inner_arg
                                        {
                                            format!("{}", s.len())
                                        } else {
                                            let c_arg = emit_expr(inner_arg);
                                            format!("sizeof({c_arg})")
                                        };
                                        return format!("((int64_t){size})");
                                    }
                                }
                                "CHAR" => return "((int64_t)1)".to_string(),
                                "CURRENT-DATE" | "WHEN-COMPILED" => {
                                    return "((int64_t)21)".to_string()
                                }
                                _ => {}
                            }
                        }
                        if let HirExpr::Literal(HirLiteral::String(s)) = arg_expr {
                            return format!("((int64_t){})", s.len());
                        }
                        let (c_arg, c_len) = string_arg_ptr_len_with_ctx(arg_expr, ctx);
                        format!("cobol_func_length({c_arg}, {c_len})")
                    } else {
                        "0".to_string()
                    }
                }
                "NUMVAL" | "NUMVAL-C" => {
                    if let Some(arg_expr) = args.first() {
                        let (c_arg, c_len) = string_arg_ptr_len_with_ctx(arg_expr, ctx);
                        format!("cobol_func_numval({c_arg}, {c_len})")
                    } else {
                        "0".to_string()
                    }
                }
                "MAX" => {
                    let has_alpha = args.iter().any(|a| {
                        matches!(a, HirExpr::Literal(HirLiteral::String(_)))
                            || is_alphanumeric_expr(a, &[])
                    });
                    if has_alpha && !args.is_empty() {
                        emit_alpha_max_min(args, "cobol_func_max_alpha")
                    } else if args
                        .iter()
                        .any(|arg| aggregate_arg_requires_double(arg, ctx))
                    {
                        let arg_list = args.iter().map(emit_expr_as_double).collect::<Vec<_>>();
                        emit_double_max_min(&arg_list, ">")
                    } else if let Some(arg_list) = emit_all_subscript_values(args, ctx, false) {
                        let count = arg_list.len();
                        let arg_list = arg_list.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_max_int_n(_mv, {}); }})",
                            count
                        )
                    } else if c_args.len() >= 2 {
                        let arg_list =
                            emit_all_subscript_values(args, ctx, false).unwrap_or(c_args);
                        let count = arg_list.len();
                        let arg_list = arg_list.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_max_int_n(_mv, {}); }})",
                            count
                        )
                    } else {
                        c_args.first().cloned().unwrap_or_else(|| "0".to_string())
                    }
                }
                "MIN" => {
                    let has_alpha = args.iter().any(|a| {
                        matches!(a, HirExpr::Literal(HirLiteral::String(_)))
                            || is_alphanumeric_expr(a, &[])
                    });
                    if has_alpha && !args.is_empty() {
                        emit_alpha_max_min(args, "cobol_func_min_alpha")
                    } else if args
                        .iter()
                        .any(|arg| aggregate_arg_requires_double(arg, ctx))
                    {
                        let arg_list = args.iter().map(emit_expr_as_double).collect::<Vec<_>>();
                        emit_double_max_min(&arg_list, "<")
                    } else if let Some(arg_list) = emit_all_subscript_values(args, ctx, false) {
                        let count = arg_list.len();
                        let arg_list = arg_list.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_min_int_n(_mv, {}); }})",
                            count
                        )
                    } else if c_args.len() >= 2 {
                        let arg_list =
                            emit_all_subscript_values(args, ctx, false).unwrap_or(c_args);
                        let count = arg_list.len();
                        let arg_list = arg_list.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_min_int_n(_mv, {}); }})",
                            count
                        )
                    } else {
                        c_args.first().cloned().unwrap_or_else(|| "0".to_string())
                    }
                }
                "MOD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_mod({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "INTEGER" => {
                    if let Some(arg_expr) = args.first() {
                        if aggregate_arg_requires_double(arg_expr, ctx)
                            || expr_requires_double_precision(arg_expr, &[])
                        {
                            let arg = emit_expr_as_double(arg_expr);
                            format!("((int64_t)floor({arg}))")
                        } else {
                            let arg = emit_expr_as_numeric(arg_expr);
                            format!("cobol_func_integer({arg}, 0)")
                        }
                    } else {
                        "0".to_string()
                    }
                }
                "INTEGER-PART" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer({arg}, 0)")
                    } else {
                        "0".to_string()
                    }
                }
                "ORD" => {
                    if let Some(arg_expr) = args.first() {
                        match arg_expr {
                            HirExpr::FunctionCall { name, args }
                                if name.eq_ignore_ascii_case("CHAR") =>
                            {
                                if let Some(arg) = args.first() {
                                    emit_expr_as_numeric(arg)
                                } else {
                                    "0".to_string()
                                }
                            }
                            HirExpr::Literal(HirLiteral::String(s)) => {
                                if let Some(ch) = s.bytes().next() {
                                    format!("cobol_func_ord({ch})")
                                } else {
                                    "cobol_func_ord(0)".to_string()
                                }
                            }
                            HirExpr::DataRef(_)
                            | HirExpr::Variable(_)
                            | HirExpr::Subscript { .. } => {
                                // Variable may be a char array; dereference
                                // the first byte.
                                let c = emit_expr(arg_expr);
                                format!("cobol_func_ord((uint8_t)*((const uint8_t*){c}))")
                            }
                            _ => {
                                if let Some(arg) = c_args.first() {
                                    format!("cobol_func_ord((uint8_t){arg})")
                                } else {
                                    "0".to_string()
                                }
                            }
                        }
                    } else {
                        "0".to_string()
                    }
                }
                "CHAR" => {
                    if let Some(arg) = c_args.first() {
                        format!("({{ static uint8_t _chbuf[2]; _chbuf[0] = cobol_func_char((uint32_t){arg}); _chbuf[1] = '\\0'; _chbuf; }})")
                    } else {
                        "0".to_string()
                    }
                }
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    // String-returning functions: copy arg to temp buffer, apply, return buffer
                    if let Some(arg_expr) = args.first() {
                        let func = match upper_name.as_str() {
                            "UPPER-CASE" => "cobol_func_upper_case",
                            "LOWER-CASE" => "cobol_func_lower_case",
                            _ => "cobol_func_reverse",
                        };
                        let (c_src, size) = emit_string_func_arg(arg_expr);
                        format!(
                            "({{ static uint8_t _sfbuf[{size}]; \
                             memcpy(_sfbuf, (const uint8_t*){c_src}, {size}); \
                             {func}(_sfbuf, {size}); _sfbuf; }})"
                        )
                    } else {
                        "((uint8_t*)0)".to_string()
                    }
                }
                "ABS" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_abs({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "SQRT" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_sqrt({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "EXP" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_exp({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "EXP10" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_exp10({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "LOG" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_log({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "LOG10" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_log10({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "SIN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_sin({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "COS" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_cos({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "TAN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_tan({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ASIN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_asin({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ACOS" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_acos({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ATAN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_atan({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "FACTORIAL" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_factorial({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "REM" | "REMAINDER" => {
                    if args.len() >= 2 {
                        let d0 = emit_expr_as_double(&args[0]);
                        let d1 = emit_expr_as_double(&args[1]);
                        format!("cobol_func_rem({d0}, {d1})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "RANDOM" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_random({arg})")
                    } else {
                        "cobol_func_random(0)".to_string()
                    }
                }
                "SIGN" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_sign({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "CEILING" | "CEIL" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_ceiling({d})")
                    } else {
                        "0".to_string()
                    }
                }
                "FLOOR" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_floor({d})")
                    } else {
                        "0".to_string()
                    }
                }
                "ANNUITY" => {
                    if args.len() >= 2 {
                        let rate = emit_expr_as_double(&args[0]);
                        let periods = emit_expr_as_numeric(&args[1]);
                        format!("cobol_func_annuity({rate}, {periods})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "STORED-CHAR-LENGTH" => {
                    if let Some(arg) = c_args.first() {
                        format!(
                            "cobol_func_stored_char_length(\
                             (const uint8_t*){arg}, sizeof({arg}))"
                        )
                    } else {
                        "0".to_string()
                    }
                }
                "MEAN" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_mean(_mv, {}); }})",
                        arg_list.len()
                    )
                }
                "MEDIAN" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_median(_mv, {}); }})",
                        arg_list.len()
                    )
                }
                "RANGE" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _rv[] = {{{joined}}}; \
                         cobol_func_range(_rv, {}); }})",
                        arg_list.len()
                    )
                }
                "MIDRANGE" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_midrange(_mv, {}); }})",
                        arg_list.len()
                    )
                }
                "STANDARD-DEVIATION" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_standard_deviation(_mv, {}); }})",
                        arg_list.len()
                    )
                }
                "VARIANCE" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_variance(_mv, {}); }})",
                        arg_list.len()
                    )
                }
                "PRESENT-VALUE" => {
                    if c_args.len() >= 2 {
                        let rate = emit_expr_as_double(&args[0]);
                        let rest: Vec<_> = args[1..].iter().map(emit_expr_as_double).collect();
                        let joined = rest.join(", ");
                        format!(
                            "({{ double _pv[] = {{{joined}}}; \
                             cobol_func_present_value({rate}, _pv, {}); }})",
                            rest.len()
                        )
                    } else {
                        "0.0".to_string()
                    }
                }
                "SUM" => {
                    let arg_list =
                        emit_all_subscript_values(args, ctx, true).unwrap_or_else(|| {
                            args.iter().map(emit_expr_as_double).collect::<Vec<_>>()
                        });
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _sv[] = {{{joined}}}; \
                         cobol_func_sum_float(_sv, {}); }})",
                        arg_list.len()
                    )
                }
                "ORD-MAX" => {
                    if let Some(expanded) = emit_ord_all_subscript(args, ctx, "cobol_func_ord_max")
                    {
                        return expanded;
                    }
                    let has_alpha = args.iter().any(|a| {
                        matches!(a, HirExpr::Literal(HirLiteral::String(_)))
                            || is_alphanumeric_expr(a, &[])
                    });
                    if has_alpha && !args.is_empty() {
                        emit_alpha_ord_max_min(args, "cobol_func_ord_max_alpha")
                    } else {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _om[] = {{{arg_list}}}; \
                             cobol_func_ord_max(_om, {}); }})",
                            c_args.len()
                        )
                    }
                }
                "ORD-MIN" => {
                    if let Some(expanded) = emit_ord_all_subscript(args, ctx, "cobol_func_ord_min")
                    {
                        return expanded;
                    }
                    let has_alpha = args.iter().any(|a| {
                        matches!(a, HirExpr::Literal(HirLiteral::String(_)))
                            || is_alphanumeric_expr(a, &[])
                    });
                    if has_alpha && !args.is_empty() {
                        emit_alpha_ord_max_min(args, "cobol_func_ord_min_alpha")
                    } else {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _om[] = {{{arg_list}}}; \
                             cobol_func_ord_min(_om, {}); }})",
                            c_args.len()
                        )
                    }
                }
                "INTEGER-OF-DATE" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer_of_date({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DATE-OF-INTEGER" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_date_of_integer({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "INTEGER-OF-DAY" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer_of_day({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DAY-OF-INTEGER" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_day_of_integer({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DATE-TO-YYYYMMDD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_date_to_yyyymmdd({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "YEAR-TO-YYYY" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_year_to_yyyy({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "DAY-TO-YYYYDDD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_day_to_yyyyddd({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "TEST-DATE-YYYYMMDD" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_test_date_yyyymmdd({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "TEST-DAY-YYYYDDD" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_test_day_yyyyddd({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "CURRENT-DATE" => {
                    "({ static uint8_t _cdbuf[22]; cobol_func_current_date(_cdbuf, 21); _cdbuf; })"
                        .to_string()
                }
                "WHEN-COMPILED" => {
                    "({ static uint8_t _wcbuf[22]; cobol_func_when_compiled(_wcbuf, 21); _wcbuf; })"
                        .to_string()
                }
                "NATIONAL-OF" => {
                    // FUNCTION NATIONAL-OF(alphanumeric-var)
                    // Returns a national value; in expression context, emit
                    // as a statement expression that fills a temp buffer.
                    if let Some(arg) = c_args.first() {
                        format!(
                            "({{ static uint16_t _ntmp[256]; \
                             cobol_func_national_of(\
                             (const uint8_t*){arg}, sizeof({arg}), \
                             _ntmp, 256); _ntmp; }})"
                        )
                    } else {
                        "((uint16_t*)0)".to_string()
                    }
                }
                "DISPLAY-OF" => {
                    // FUNCTION DISPLAY-OF(national-var)
                    // Returns an alphanumeric value.
                    if let Some(arg) = c_args.first() {
                        format!(
                            "({{ static char _dtmp[256]; \
                             cobol_func_display_of(\
                             (const uint16_t*){arg}, sizeof({arg})/sizeof(uint16_t), \
                             (uint8_t*)_dtmp, 256); _dtmp; }})"
                        )
                    } else {
                        "((char*)0)".to_string()
                    }
                }
                _ => {
                    // User-defined or unhandled intrinsic function:
                    // use cobol_func_ prefix with lowercase name so it matches
                    // the runtime function naming convention.
                    let c_name = sanitize_name(name).to_lowercase();
                    format!("cobol_func_{c_name}({})", c_args.join(", "))
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length: _,
        } => {
            // In numeric expression context, reference modification returns
            // a pointer expression. This is unusual but we emit it for
            // completeness. Callers like emit_display_operand handle the
            // display case directly.
            let c_var = data_name_to_c_name(variable);
            let c_start = emit_expr_as_numeric(start);
            format!("({c_var} + ({c_start} - 1))")
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => emit_subscript_access(variable, subscripts),
    }
}

fn string_arg_ptr_len_with_ctx(expr: &HirExpr, ctx: &CodegenContext) -> (String, String) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            (
                format!("(const uint8_t*)\"{escaped}\""),
                s.len().to_string(),
            )
        }
        HirExpr::Literal(HirLiteral::Space) => {
            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            ("(const uint8_t*)\"\\\"\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            ("(const uint8_t*)\"\\x00\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            ("(const uint8_t*)\"\\xFF\"".to_string(), "1".to_string())
        }
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            let c_expr = emit_expr_with_ctx(expr, ctx);
            let base_name = expr_data_name(expr)
                .map(data_name_to_c_name)
                .unwrap_or_default();
            let leaf_name = extract_leaf_member(&c_expr);
            let len = ctx
                .data_item_size(&base_name)
                .or_else(|| ctx.data_item_size(leaf_name))
                .or_else(|| ctx.display_numeric_size(&base_name))
                .or_else(|| ctx.display_numeric_size(leaf_name))
                .unwrap_or(0);
            let ptr = if (!base_name.is_empty() && ctx.is_group_name(&base_name))
                || ctx.is_group_name(leaf_name)
            {
                format!("(const uint8_t*)&({c_expr})")
            } else {
                format!("(const uint8_t*){c_expr}")
            };
            (ptr, len.to_string())
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_fn = name.to_uppercase();
            match upper_fn.as_str() {
                "CHAR" => {
                    let e = emit_expr_with_ctx(expr, ctx);
                    (format!("(const uint8_t*){e}"), "1".to_string())
                }
                "CURRENT-DATE" | "WHEN-COMPILED" => {
                    let e = emit_expr_with_ctx(expr, ctx);
                    (format!("(const uint8_t*){e}"), "21".to_string())
                }
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    let e = emit_expr_with_ctx(expr, ctx);
                    let len = args
                        .first()
                        .map(|arg| string_arg_ptr_len_with_ctx(arg, ctx).1)
                        .unwrap_or_else(|| "0".to_string());
                    (format!("(const uint8_t*){e}"), len)
                }
                _ => {
                    let e = emit_expr_with_ctx(expr, ctx);
                    (format!("(const uint8_t*)&({e})"), format!("sizeof({e})"))
                }
            }
        }
        _ => {
            let e = emit_expr_with_ctx(expr, ctx);
            (format!("(const uint8_t*)&({e})"), format!("sizeof({e})"))
        }
    }
}

fn emit_ord_all_subscript(args: &[HirExpr], ctx: &CodegenContext, func: &str) -> Option<String> {
    let values = emit_all_subscript_values(args, ctx, false)?;
    let count = values.len();
    let values = values.join(", ");
    Some(format!(
        "({{ int64_t _om[] = {{{values}}}; {func}(_om, {count}); }})"
    ))
}

fn emit_all_subscript_values(
    args: &[HirExpr],
    ctx: &CodegenContext,
    as_double: bool,
) -> Option<Vec<String>> {
    let [arg] = args else {
        return None;
    };

    match arg {
        HirExpr::Subscript {
            variable,
            subscripts,
        } if matches!(subscripts.as_slice(), [sub] if is_all_subscript_marker(sub)) => {
            let base_name = data_name_to_c_name(variable);
            let count = ctx
                .occurs_count(&base_name)
                .or_else(|| ctx.occurs_count(extract_leaf_member(&base_name)))?;
            Some(
                (1..=count)
                    .map(|idx| {
                        let element = HirExpr::Subscript {
                            variable: variable.clone(),
                            subscripts: vec![HirExpr::Literal(HirLiteral::Integer(idx as i64))],
                        };
                        if as_double {
                            super::emit_expr_as_double_with_ctx(&element, ctx)
                        } else {
                            super::emit_expr_as_numeric_with_ctx(&element, ctx)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
        HirExpr::DataRef(data_ref) if matches!(data_ref.subscripts.as_slice(), [sub] if is_all_subscript_marker(sub)) =>
        {
            let base_name = data_name_to_c_name(&data_ref.name);
            let count = ctx
                .occurs_count(&base_name)
                .or_else(|| ctx.occurs_count(extract_leaf_member(&base_name)))?;
            Some(
                (1..=count)
                    .map(|idx| {
                        let mut element_ref = data_ref.clone();
                        element_ref.subscripts =
                            vec![HirExpr::Literal(HirLiteral::Integer(idx as i64))];
                        let element = HirExpr::DataRef(element_ref);
                        if as_double {
                            super::emit_expr_as_double_with_ctx(&element, ctx)
                        } else {
                            super::emit_expr_as_numeric_with_ctx(&element, ctx)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
        _ => None,
    }
}

fn emit_double_max_min(args: &[String], cmp: &str) -> String {
    let count = args.len();
    if count == 0 {
        return "0.0".to_string();
    }
    let joined = args.join(", ");
    format!(
        "({{ double _mv[] = {{{joined}}}; double _m = _mv[0]; for (int _i = 1; _i < {count}; _i++) {{ if (_mv[_i] {cmp} _m) _m = _mv[_i]; }} _m; }})"
    )
}

fn aggregate_arg_requires_double(expr: &HirExpr, ctx: &CodegenContext) -> bool {
    if expr_contains_decimal(expr) {
        return true;
    }
    match expr {
        HirExpr::DataRef(data_ref) => {
            let base_name = data_name_to_c_name(&data_ref.name);
            ctx.is_decimal_name(&base_name)
                || (!is_qualified_c_name(&base_name)
                    && ctx.is_decimal_name(extract_leaf_member(&base_name)))
        }
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            let base_name = data_name_to_c_name(name);
            ctx.is_decimal_name(&base_name)
                || (!is_qualified_c_name(&base_name)
                    && ctx.is_decimal_name(extract_leaf_member(&base_name)))
        }
        HirExpr::UnaryOp { operand, .. } => aggregate_arg_requires_double(operand, ctx),
        HirExpr::BinaryOp { left, right, .. } => {
            aggregate_arg_requires_double(left, ctx) || aggregate_arg_requires_double(right, ctx)
        }
        HirExpr::FunctionCall { name, .. } => intrinsic_returns_double(name),
        _ => false,
    }
}

fn is_all_subscript_marker(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Literal(HirLiteral::Space) => true,
        HirExpr::Literal(HirLiteral::String(s)) => s.trim().is_empty(),
        HirExpr::Literal(HirLiteral::AllChar(s)) => s.trim().is_empty(),
        _ => false,
    }
}

/// Generates C code for subscripted table access.
/// COBOL subscripts are 1-based; C arrays are 0-based.
///
/// For items inside groups with nested OCCURS, generates proper C struct
/// access paths with subscripts at each OCCURS level.  For example:
///   01 TABLE-1. 05 GRP OCCURS 3. 10 ITEM PIC 9 OCCURS 4.
/// `ITEM(I, J)` becomes:
///   `TABLE_1.members._m_GRP[(I)-1].members._m_ITEM[(J)-1]`
pub(crate) fn emit_subscript_access(variable: &HirDataName, subscripts: &[HirExpr]) -> String {
    let c_name = data_name_to_c_name(variable);
    // Check if we have pre-computed path info for this variable (nested OCCURS)
    let path_info = with_active_context(|ctx| {
        ctx.subscript_path(&c_name)
            .or_else(|| ctx.subscript_path(extract_leaf_member(&c_name)))
    });

    if let Some(ref info) = path_info {
        let occurs_count = info.segments.iter().filter(|(_, has)| *has).count();
        if occurs_count > 0 && subscripts.len() >= occurs_count {
            // Build the full struct access path, inserting subscripts at OCCURS levels
            let mut access = info.root.clone();
            let mut sub_idx = 0;
            for (segment_suffix, has_occurs) in &info.segments {
                access.push_str(segment_suffix);
                if *has_occurs && sub_idx < subscripts.len() {
                    let idx = emit_expr_as_numeric(&subscripts[sub_idx]);
                    access.push_str(&format!("[({idx}) - 1]"));
                    sub_idx += 1;
                }
            }
            return access;
        }
    }

    // Fallback: simple flat array subscript (top-level OCCURS without group nesting)
    if subscripts.len() == 1 {
        let idx = emit_expr_as_numeric(&subscripts[0]);
        format!("{c_name}[({idx}) - 1]")
    } else {
        let mut access = c_name;
        for sub in subscripts {
            let idx = emit_expr_as_numeric(sub);
            access = format!("{access}[({idx}) - 1]");
        }
        access
    }
}
/// Returns true if the given HirType requires CobolDecimal representation
/// (i.e., has fractional decimal places).
pub(crate) fn needs_decimal(data_type: &HirType) -> bool {
    matches!(
        data_type,
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0
    ) || matches!(data_type, HirType::Comp3 { decimal_places, .. } if *decimal_places > 0)
}

/// Check if a variable is a USAGE DISPLAY numeric stored as char[] inside a group.
/// Returns Some(display_size) if so, None otherwise.
/// Top-level numeric items are stored as int64_t and return None.
pub(crate) fn grp_display_size(c_name: &str, data_items: &[HirDataItem]) -> Option<u32> {
    let is_simple_name =
        !c_name.contains("__") && !c_name.contains("._m_") && !c_name.contains('[');
    if is_simple_name {
        let lookup = extract_leaf_member(c_name);
        let display_size = with_active_context(|ctx| ctx.display_numeric_size(lookup));
        if display_size.is_some() {
            return display_size;
        }
    }

    // Handle qualified names like "WS_DST__FIELD_A" by extracting the member
    // part after the last "__".
    let base_name = if c_name.contains(".members._m_") {
        extract_leaf_member(c_name)
    } else {
        c_name
            .rfind("__")
            .map(|pos| &c_name[pos + 2..])
            .unwrap_or(c_name)
    };
    // Strip any trailing subscripts: NAME[...] -> NAME
    let base_name = base_name.split('[').next().unwrap_or(base_name);
    // Search within groups first — if it exists as a display numeric
    // member of any group, it's stored as char[].
    fn search_in(c_name: &str, members: &[HirDataItem]) -> Option<u32> {
        for m in members {
            if m.redefines.is_some() {
                continue;
            }
            let mc = sanitize_name(&m.name);
            if mc == c_name {
                if let HirType::Numeric { size, .. } = &m.data_type {
                    return Some(*size);
                }
            }
            if let HirType::Group {
                members: sub_members,
                ..
            } = &m.data_type
            {
                if let Some(s) = search_in(c_name, sub_members) {
                    return Some(s);
                }
            }
        }
        None
    }
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(s) = search_in(base_name, members) {
                return Some(s);
            }
        }
    }
    // Fallback: check the precomputed display numeric map (covers RENAMES etc.)
    with_active_context(|ctx| ctx.display_numeric_size(base_name))
}

/// Extract the leaf member name from a C target expression for type lookup.
/// For simple names like `FOO`, returns `"FOO"`.
/// For struct paths like `GRP.members._m_X[i].members._m_Y`, returns `"Y"`.
/// For qualified macro names like `GRP__FIELD`, returns `"FIELD"`.
pub(crate) fn extract_leaf_member(c_target: &str) -> &str {
    // Find the last `._m_` segment (possibly followed by `[...]`)
    if let Some(pos) = c_target.rfind("._m_") {
        let after = &c_target[pos + 4..];
        // Strip any trailing subscript `[...]`
        after.split('[').next().unwrap_or(after)
    } else if let Some(pos) = c_target.rfind("__") {
        // Qualified macro name: WS_GRP__FIELD -> FIELD
        // Strip any trailing subscripts: NUMBER2[...] -> NUMBER2
        let after = &c_target[pos + 2..];
        after.split('[').next().unwrap_or(after)
    } else {
        // Simple name, strip subscripts
        c_target.split('[').next().unwrap_or(c_target)
    }
}

/// Generate C code to store an int64_t value into a target variable.
/// If the target is a display numeric group member (char[]), uses
/// cobol_store_numeric_display. Otherwise emits direct assignment.
/// `c_target` is the full C expression (may include subscripts).
/// `base_name` is the sanitized COBOL variable name for type lookup.
pub(crate) fn emit_store_int(
    out: &mut String,
    c_target: &str,
    value_expr: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_item = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items));
    let stored_value_expr = target_item
        .filter(|item| item.scale_adjustment != 0)
        .map(|item| apply_scale_adjustment_to_store(value_expr, item.scale_adjustment))
        .unwrap_or_else(|| value_expr.to_string());
    let stored_value_expr = target_item
        .filter(|item| {
            !item.is_numeric_edited
                && matches!(
                    item.data_type,
                    HirType::Numeric {
                        decimal_places: 0,
                        ..
                    } | HirType::Binary { .. }
                )
        })
        .map(|item| truncate_integral_to_picture_size(&stored_value_expr, item))
        .unwrap_or(stored_value_expr);
    let stored_value_expr = if target_item.is_some_and(is_unsigned_numeric_storage) {
        format!("llabs({stored_value_expr})")
    } else {
        stored_value_expr
    };
    if let Some(item) = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .filter(|item| item.is_numeric_edited)
    {
        let pic = item
            .picture
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "9".to_string());
        let escaped_pic = escape_c_string(&pic);
        let pic_len = pic.len();
        let tgt_size = find_data_item_size(c_target, data_items);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _ned = {{ .value = ({stored_value_expr}), .scale = 0, .size = {tgt_size}, .is_signed = 1 }}; \
             char _ned_buf[256]; uint32_t _ned_len = cobol_decimal_to_display(&_ned, (uint8_t*)_ned_buf, 256, \
             (const uint8_t*)\"{escaped_pic}\", {pic_len}); cobol_move_string((const uint8_t*)_ned_buf, _ned_len, (uint8_t*){c_target}, {tgt_size}); }}\n"
        ));
    } else if let Some(disp_size) = grp_display_size(c_target, data_items) {
        let c_target_ptr = display_numeric_ptr(c_target);
        out.push_str(&format!(
            "{pad}cobol_store_numeric_display({stored_value_expr}, {c_target_ptr}, {disp_size});\n"
        ));
    } else if find_data_item(c_target, data_items)
        .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }))
    {
        let tgt_size = find_data_item_size(c_target, data_items);
        out.push_str(&format!(
            "{pad}cobol_move_numeric_to_display({stored_value_expr}, 0, (uint8_t*)&{c_target}, {tgt_size});\n"
        ));
    } else if is_group_member_field(c_target) {
        out.push_str(&format!(
            "{pad}cobol_move_numeric_to_display({stored_value_expr}, 0, (uint8_t*){c_target}, sizeof({c_target}));\n"
        ));
    } else {
        out.push_str(&format!("{pad}{c_target} = {stored_value_expr};\n"));
    }
}

fn truncate_integral_to_picture_size(value_expr: &str, item: &HirDataItem) -> String {
    let size = match item.data_type {
        HirType::Numeric { size, .. } | HirType::Binary { size } => size,
        _ => return value_expr.to_string(),
    };
    let size = if item.scale_adjustment > 0 {
        size.saturating_sub(item.scale_adjustment as u32)
    } else {
        size
    };
    if size == 0 || size > 18 {
        return value_expr.to_string();
    }
    let factor = pow10_i64_literal(size);
    let int_value = format!("((int64_t)({value_expr}))");
    format!("(({int_value} >= 0) ? ({int_value} % {factor}) : -((-{int_value}) % {factor}))")
}

fn is_unsigned_numeric_storage(item: &HirDataItem) -> bool {
    if item.is_numeric_edited {
        return false;
    }
    match item.data_type {
        HirType::Numeric {
            is_signed: false, ..
        }
        | HirType::Comp3 { .. }
        | HirType::Binary { .. } => item
            .picture
            .as_ref()
            .is_some_and(|pic| !pic.to_ascii_uppercase().contains('S')),
        _ => false,
    }
}

pub(crate) fn apply_scale_adjustment_to_read(value_expr: &str, adjustment: i32) -> String {
    if adjustment > 0 {
        format!(
            "(({value_expr}) * {})",
            pow10_i64_literal(adjustment as u32)
        )
    } else if adjustment < 0 {
        format!(
            "(({value_expr}) / {})",
            pow10_i64_literal((-adjustment) as u32)
        )
    } else {
        value_expr.to_string()
    }
}

pub(crate) fn apply_scale_adjustment_to_store(value_expr: &str, adjustment: i32) -> String {
    if adjustment > 0 {
        format!(
            "(({value_expr}) / {})",
            pow10_i64_literal(adjustment as u32)
        )
    } else if adjustment < 0 {
        format!(
            "(({value_expr}) * {})",
            pow10_i64_literal((-adjustment) as u32)
        )
    } else {
        value_expr.to_string()
    }
}

fn pow10_i64_literal(exp: u32) -> String {
    10_i64.pow(exp).to_string()
}

/// Check whether a variable name refers to a group member (char[] without null
/// terminator). Returns true for variables that are Alphanumeric or Display
/// Numeric members of groups.
pub(crate) fn is_group_member_field(c_target: &str) -> bool {
    let base = extract_leaf_member(c_target);
    with_active_context(|ctx| ctx.is_group_alpha_name(base) || ctx.has_display_numeric(base))
}

/// Generate C code to store `target op value` (e.g., += , -=, *=, /=).
pub(crate) fn emit_store_int_op(
    out: &mut String,
    c_target: &str,
    op: &str,
    value_expr: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(item) = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .filter(|item| item.is_numeric_edited)
    {
        let current = format!(
            "cobol_func_numval((const uint8_t*){c_target}, {})",
            find_data_item_size(c_target, data_items)
        );
        let value = format!("({current}) {op} ({value_expr})");
        let pic = item
            .picture
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "9".to_string());
        let escaped_pic = escape_c_string(&pic);
        let pic_len = pic.len();
        let tgt_size = find_data_item_size(c_target, data_items);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _ned = {{ .value = ({value}), .scale = 0, .size = {tgt_size}, .is_signed = 1 }}; \
             char _ned_buf[256]; uint32_t _ned_len = cobol_decimal_to_display(&_ned, (uint8_t*)_ned_buf, 256, \
             (const uint8_t*)\"{escaped_pic}\", {pic_len}); cobol_move_string((const uint8_t*)_ned_buf, _ned_len, (uint8_t*){c_target}, {tgt_size}); }}\n"
        ));
    } else if let Some(disp_size) = grp_display_size(c_target, data_items) {
        let c_target_const_ptr = display_numeric_const_ptr(c_target);
        let c_target_ptr = display_numeric_ptr(c_target);
        out.push_str(&format!(
            "{pad}cobol_store_numeric_display(\
             cobol_display_to_int64({c_target_const_ptr}, {disp_size}) {op} ({value_expr}), \
             {c_target_ptr}, {disp_size});\n"
        ));
    } else if is_group_member_field(c_target) {
        let c_target_const_ptr = display_numeric_const_ptr(c_target);
        let c_target_ptr = display_numeric_ptr(c_target);
        out.push_str(&format!(
            "{pad}cobol_store_numeric_display(\
             cobol_display_to_int64({c_target_const_ptr}, sizeof({c_target})) {op} ({value_expr}), \
             {c_target_ptr}, sizeof({c_target}));\n"
        ));
    } else {
        out.push_str(&format!("{pad}{c_target} {op}= {value_expr};\n"));
    }
}

/// Returns true if the given expression refers to a CobolDecimal variable.
pub(crate) fn is_decimal_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => {
            if data_ref.refmod.is_some() || expr_name_is_display_numeric(&data_ref.name) {
                return false;
            }
            if expr_name_needs_decimal(&data_ref.name, data_items) {
                return true;
            }
            false
        }
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            if expr_name_is_display_numeric(name) {
                return false;
            }
            if expr_name_needs_decimal(name, data_items) {
                return true;
            }
            false
        }
        _ => false,
    }
}

fn expr_name_is_display_numeric(name: &HirDataName) -> bool {
    let c_name = data_name_to_c_name(name);
    with_active_context(|ctx| {
        ctx.has_display_numeric(&c_name) || ctx.has_display_numeric(extract_leaf_member(&c_name))
    })
}

fn expr_name_needs_decimal(name: &HirDataName, data_items: &[HirDataItem]) -> bool {
    let c_name = data_name_to_c_name(name);
    find_data_item_for_expr_name(name, data_items).is_some_and(|i| needs_decimal(&i.data_type))
        || with_active_context(|ctx| {
            ctx.is_decimal_name(&c_name)
                || (!is_qualified_c_name(&c_name)
                    && ctx.is_decimal_name(extract_leaf_member(&c_name)))
        })
}

fn find_data_item_for_expr_name<'a>(
    name: &HirDataName,
    data_items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    let c_name = data_name_to_c_name(name);
    let exact = find_data_item_by_name(name, data_items)
        .or_else(|| find_data_item(&c_name, data_items))
        .or_else(|| find_data_item_by_c_name(&c_name, data_items));
    if exact.is_some() || is_qualified_c_name(&c_name) {
        return exact;
    }
    find_original_data_item_by_sanitized_name(extract_leaf_member(&c_name), data_items)
}

fn is_qualified_c_name(c_name: &str) -> bool {
    c_name.contains("__") || c_name.contains("._m_") || c_name.contains(".members.")
}

/// Check whether an expression refers to a group variable (emitted as a C union).
pub(crate) fn is_group_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => find_data_item_by_name(&data_ref.name, data_items)
            .is_some_and(|i| matches!(i.data_type, HirType::Group { .. })),
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            find_data_item_by_name(name, data_items)
                .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }))
        }
        _ => false,
    }
}

/// Check whether an expression refers to an alphanumeric variable (emitted as `char[]`).
pub(crate) fn is_alpha_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => find_data_item_by_name(&data_ref.name, data_items)
            .is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. })),
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            find_data_item_by_name(name, data_items)
                .is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }))
        }
        _ => false,
    }
}

/// Check whether an expression tree contains any decimal variable or decimal
/// literal, meaning that converting to int64 would lose fractional precision.
pub(crate) fn expr_contains_decimal(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => {
            let c_name = data_name_to_c_name(&data_ref.name);
            with_active_context(|ctx| ctx.is_decimal_name(&c_name))
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            with_active_context(|ctx| ctx.is_decimal_name(&c_name))
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => decimal_literal_has_fraction(d),
        HirExpr::BinaryOp { left, right, .. } => {
            expr_contains_decimal(left) || expr_contains_decimal(right)
        }
        HirExpr::UnaryOp { operand, .. } => expr_contains_decimal(operand),
        HirExpr::FunctionCall { args, .. } => args.iter().any(expr_contains_decimal),
        _ => false,
    }
}

fn decimal_literal_has_fraction(literal: &str) -> bool {
    literal.contains('.')
}

/// Emit an expression as int64, converting CobolDecimal to int64 if needed.
/// For simple variable/subscript expressions, wraps with cobol_decimal_to_int64.
/// For compound expressions (BinaryOp etc), recursively converts sub-expressions.
pub(crate) fn emit_int_compatible_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> String {
    match expr {
        HirExpr::DataRef(data_ref) if data_ref.refmod.is_some() => {
            let (ptr, len) = emit_alphanumeric_operand(expr, data_items);
            format!("cobol_func_numval({ptr}, {len})")
        }
        HirExpr::ReferenceModification { .. } => {
            let (ptr, len) = emit_alphanumeric_operand(expr, data_items);
            format!("cobol_func_numval({ptr}, {len})")
        }
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            if is_decimal_expr(expr, data_items) {
                let c = emit_expr(expr);
                format!("cobol_decimal_to_int64(&{c})")
            } else if is_group_expr(expr, data_items) {
                // Group variables are C unions; convert via cobol_func_numval
                // (treat group bytes as alphanumeric and parse as a number).
                let c = emit_expr(expr);
                let size = find_data_item_size(&c, data_items);
                format!("cobol_func_numval((const uint8_t*)&{c}, {size})")
            } else if is_alpha_expr(expr, data_items) {
                // Alphanumeric fields are char[] in C; convert to int via numval.
                let c = emit_expr(expr);
                let size = find_data_item_size(&c, data_items);
                format!("cobol_func_numval((const uint8_t*){c}, {size})")
            } else {
                // Check if this is a display numeric stored as char[] in a group
                let c = emit_expr(expr);
                let c_var = expr_data_name(expr).map(data_name_to_c_name);
                if let Some(disp_size) = grp_display_size(&c, data_items).or_else(|| {
                    c_var
                        .as_deref()
                        .and_then(|name| grp_display_size(name, data_items))
                }) {
                    let c_ptr = display_numeric_const_ptr(&c);
                    format!("cobol_display_to_int64({c_ptr}, {disp_size})")
                } else {
                    let adjustment = expr_data_name(expr)
                        .and_then(|name| find_data_item_by_name(name, data_items))
                        .map_or(0, |item| item.scale_adjustment);
                    apply_scale_adjustment_to_read(&c, adjustment)
                }
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = emit_int_compatible_expr(left, data_items);
            let r = emit_int_compatible_expr(right, data_items);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("((int64_t)pow((double){l}, (double){r}))"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_int_compatible_expr(operand, data_items);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::FunctionCall { .. } => {
            // FunctionCall results are always numeric C types (int64_t/double).
            // emit_expr now uses emit_expr_as_numeric for function arguments,
            // which auto-converts CobolDecimal variables via the DECIMAL_NAMES
            // thread-local, so we can safely delegate.
            emit_expr(expr)
        }
        _ => emit_expr(expr),
    }
}

/// Emit code to assign a value to a CobolDecimal target.
/// Handles integer literals, decimal literals, Zero, CobolDecimal sources,
/// and integer variable sources.
pub(crate) fn emit_assign_to_decimal(
    out: &mut String,
    from: &HirExpr,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_scale_adjustment = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .map_or(0, |item| item.scale_adjustment);
    if let Some((target_size, target_scale)) = display_numeric_c_expr_info(c_target, data_items) {
        let c_target_ptr = display_numeric_ptr(c_target);
        match from {
            HirExpr::Literal(HirLiteral::Integer(n)) => {
                let scaled = scale_decimal_to_target(*n, 0, target_scale);
                out.push_str(&format!(
                    "{pad}cobol_store_numeric_display({scaled}, {c_target_ptr}, {target_size});\n"
                ));
            }
            HirExpr::Literal(HirLiteral::Decimal(d)) => {
                let (scaled, scale) = parse_decimal_literal(d);
                let scaled = scale_decimal_to_target(scaled, scale, target_scale);
                out.push_str(&format!(
                    "{pad}cobol_store_numeric_display({scaled}, {c_target_ptr}, {target_size});\n"
                ));
            }
            HirExpr::Literal(HirLiteral::Zero) | HirExpr::Literal(HirLiteral::Null) => {
                out.push_str(&format!(
                    "{pad}cobol_store_numeric_display(0, {c_target_ptr}, {target_size});\n"
                ));
            }
            _ => {
                let init = decimal_temp_init_from_expr("_src_dec", from, data_items);
                out.push_str(&format!(
                    "{pad}{{ {init} cobol_store_numeric_display(_src_dec.value, {c_target_ptr}, {target_size}); }}\n"
                ));
            }
        }
        return;
    }

    match from {
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            emit_assign_scaled_int_to_decimal(
                out,
                c_target,
                &n.to_string(),
                "0",
                target_scale_adjustment,
                pad,
            );
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let (scaled, scale) = parse_decimal_literal(d);
            emit_assign_scaled_int_to_decimal(
                out,
                c_target,
                &scaled.to_string(),
                &scale.to_string(),
                target_scale_adjustment,
                pad,
            );
        }
        HirExpr::Literal(HirLiteral::Zero) | HirExpr::Literal(HirLiteral::Null) => {
            // Only zero the value; preserve scale/size/is_signed so that
            // subsequent double-precision arithmetic (cobol_decimal_to_double,
            // cobol_decimal_from_double) still knows the field's precision.
            out.push_str(&format!("{pad}{c_target}.value = 0;\n"));
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_decimal_from_string(\
                 (const uint8_t*)\"{escaped}\", {len}, &{c_target});\n"
            ));
        }
        _ => {
            if is_decimal_expr(from, data_items) {
                // Preserve the target PICTURE metadata; only move the scaled value.
                let c_src = emit_expr(from);
                emit_assign_decimal_value_to_decimal(
                    out,
                    c_target,
                    &c_src,
                    target_scale_adjustment,
                    pad,
                );
            } else if expr_requires_double_precision(from, data_items) {
                // Expression contains decimal sub-expressions or fractional
                // literals: use double arithmetic to preserve precision, then
                // convert back via cobol_decimal_from_double which respects the
                // target's existing scale.
                let e = emit_expr_as_double(from);
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_double({e}, &{c_target});\n"
                ));
            } else {
                // Integer variable or expression -> CobolDecimal. Preserve
                // the target PICTURE metadata and scale the integer value into it.
                let e = emit_int_compatible_expr(from, data_items);
                emit_assign_scaled_int_to_decimal(
                    out,
                    c_target,
                    &e,
                    "0",
                    target_scale_adjustment,
                    pad,
                );
            }
        }
    }
}

fn emit_assign_scaled_int_to_decimal(
    out: &mut String,
    c_target: &str,
    scaled_value_expr: &str,
    source_scale_expr: &str,
    target_scale_adjustment: i32,
    pad: &str,
) {
    let leading_p_limit =
        leading_p_decimal_value_limit_statement(c_target, target_scale_adjustment);
    out.push_str(&format!(
        "{pad}{{ int64_t _dv = ({scaled_value_expr}); int32_t _ds = ({source_scale_expr}); \
         if ({c_target}.size == 0 && {c_target}.scale == 0) {{ \
             {c_target}.scale = _ds; {c_target}.size = 18; {c_target}.is_signed = 1; \
         }} \
         if ({c_target}.size > {c_target}.scale && {c_target}.size <= 18 && _ds >= 0) {{ \
             int32_t _keep = {c_target}.size - {c_target}.scale + _ds; \
             if (_keep > 0 && _keep <= 18) {{ \
                 int64_t _limit = 1; \
                 for (int32_t _i = 0; _i < _keep; _i++) _limit *= 10; \
                 _dv = (_dv >= 0) ? (_dv % _limit) : -((-_dv) % _limit); \
             }} \
         }} \
         int32_t _dd = {c_target}.scale - _ds; \
         if (_dd > 0) _dv *= (int64_t)pow(10.0, _dd); \
         else if (_dd < 0) _dv /= (int64_t)pow(10.0, -_dd); \
         {leading_p_limit} \
         if ({c_target}.size > 0 && {c_target}.size <= 18) {{ \
             int64_t _limit = 1; \
             for (int32_t _i = 0; _i < {c_target}.size; _i++) _limit *= 10; \
             _dv = (_dv >= 0) ? (_dv % _limit) : -((-_dv) % _limit); \
         }} \
         {c_target}.value = _dv; }}\n"
    ));
}

fn emit_assign_decimal_value_to_decimal(
    out: &mut String,
    c_target: &str,
    c_source: &str,
    target_scale_adjustment: i32,
    pad: &str,
) {
    let leading_p_limit =
        leading_p_decimal_value_limit_statement(c_target, target_scale_adjustment);
    out.push_str(&format!(
        "{pad}{{ int64_t _dv = {c_source}.value; int32_t _dd = {c_target}.scale - {c_source}.scale; \
         if ({c_target}.size == 0 && {c_target}.scale == 0) {{ \
             {c_target}.scale = {c_source}.scale; {c_target}.size = {c_source}.size; \
             {c_target}.is_signed = {c_source}.is_signed; _dd = 0; \
         }} \
         if ({c_target}.size > {c_target}.scale && {c_target}.size <= 18 && {c_source}.scale >= 0) {{ \
             int32_t _keep = {c_target}.size - {c_target}.scale + {c_source}.scale; \
             if (_keep > 0 && _keep <= 18) {{ \
                 int64_t _limit = 1; \
                 for (int32_t _i = 0; _i < _keep; _i++) _limit *= 10; \
                 _dv = (_dv >= 0) ? (_dv % _limit) : -((-_dv) % _limit); \
             }} \
         }} \
         if (_dd > 0) _dv *= (int64_t)pow(10.0, _dd); \
         else if (_dd < 0) _dv /= (int64_t)pow(10.0, -_dd); \
         {leading_p_limit} \
         if ({c_target}.size > 0 && {c_target}.size <= 18) {{ \
             int64_t _limit = 1; \
             for (int32_t _i = 0; _i < {c_target}.size; _i++) _limit *= 10; \
             _dv = (_dv >= 0) ? (_dv % _limit) : -((-_dv) % _limit); \
         }} \
         {c_target}.value = _dv; }}\n"
    ));
}

fn leading_p_decimal_value_limit_statement(c_target: &str, target_scale_adjustment: i32) -> String {
    if target_scale_adjustment >= 0 {
        return String::new();
    }
    let leading_p_count = -target_scale_adjustment;
    format!(
        "if ({c_target}.size > {leading_p_count} && {c_target}.size <= 18) {{ \
             int32_t _visible = {c_target}.size - {leading_p_count}; \
             int64_t _p_limit = 1; \
             for (int32_t _i = 0; _i < _visible; _i++) _p_limit *= 10; \
             _dv = (_dv >= 0) ? (_dv % _p_limit) : -((-_dv) % _p_limit); \
         }}"
    )
}

fn intrinsic_returns_double(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "SQRT"
            | "EXP"
            | "EXP10"
            | "LOG"
            | "LOG10"
            | "SIN"
            | "COS"
            | "TAN"
            | "ASIN"
            | "ACOS"
            | "ATAN"
            | "REM"
            | "REMAINDER"
            | "NUMVAL"
            | "NUMVAL-C"
            | "ANNUITY"
            | "PRESENT-VALUE"
            | "MEAN"
            | "MEDIAN"
            | "RANGE"
            | "MIDRANGE"
            | "STANDARD-DEVIATION"
            | "SUM"
            | "VARIANCE"
    )
}

pub(crate) fn expr_requires_double_precision(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::Literal(HirLiteral::Decimal(d)) => decimal_literal_has_fraction(d),
        HirExpr::FunctionCall { name, args } => {
            let upper_name = name.to_ascii_uppercase();
            intrinsic_returns_double(name)
                || matches!(upper_name.as_str(), "MAX" | "MIN")
                    && args.iter().any(|arg| is_decimal_expr(arg, data_items))
                || args
                    .iter()
                    .any(|arg| expr_requires_double_precision(arg, data_items))
        }
        HirExpr::UnaryOp { operand, .. } => expr_requires_double_precision(operand, data_items),
        HirExpr::BinaryOp { left, right, .. } => {
            expr_requires_double_precision(left, data_items)
                || expr_requires_double_precision(right, data_items)
                || expr_contains_decimal(expr)
        }
        _ => expr_contains_decimal(expr) && !is_decimal_expr(expr, data_items),
    }
}

pub(crate) fn expr_data_name(expr: &HirExpr) -> Option<&HirDataName> {
    match expr {
        HirExpr::DataRef(data_ref) => Some(&data_ref.name),
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => Some(name),
        _ => None,
    }
}

/// Look up a data item by name (searching flattened items including group members).
pub(crate) fn find_data_item(
    name: impl AsRef<str>,
    data_items: &[HirDataItem],
) -> Option<&HirDataItem> {
    let name = name.as_ref();
    // Handle qualified names like "WS-DST::FIELD-A"
    if let Some(pos) = name.find("::") {
        let group_name = &name[..pos];
        let member_name = &name[pos + 2..];
        // Find the group, then search within it
        for item in data_items {
            if item.name.as_str() == group_name || sanitize_name(&item.name) == group_name {
                if let HirType::Group { members, .. } = &item.data_type {
                    return find_data_item(member_name, members);
                }
            }
        }
        // Fallback: try unqualified search
        return find_data_item(member_name, data_items);
    }
    for item in data_items {
        if item.name.as_str() == name || sanitize_name(&item.name) == name {
            return Some(item);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_data_item(name, members) {
                return Some(found);
            }
        }
    }
    None
}

/// Check if a name refers to an INDEX variable (declared via INDEXED BY).
pub(crate) fn is_index_name(name: &str, data_items: &[HirDataItem]) -> bool {
    for item in data_items {
        for idx_name in &item.indexed_by {
            if idx_name.as_str() == name {
                return true;
            }
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if is_index_name(name, members) {
                return true;
            }
        }
    }
    false
}

/// Check if a sanitized C variable name corresponds to a group item.
pub(crate) fn is_group_item_c(c_name: &str, data_items: &[HirDataItem]) -> bool {
    find_data_item_by_c_name(c_name, data_items)
        .is_some_and(|item| matches!(item.data_type, HirType::Group { .. }))
}

/// Check if a sanitized C variable name corresponds to a numeric, binary,
/// comp3, index, or other non-array type stored as int64_t/CobolDecimal.
pub(crate) fn is_numeric_item_c(c_name: &str, data_items: &[HirDataItem]) -> bool {
    find_data_item_by_c_name(c_name, data_items).is_some_and(|item| {
        matches!(
            item.data_type,
            HirType::Numeric { .. }
                | HirType::Comp3 { .. }
                | HirType::Binary { .. }
                | HirType::Index
                | HirType::FloatShort
                | HirType::FloatLong
                | HirType::FloatExtended
                | HirType::Boolean
        )
    })
}

/// Return a C expression suitable for use in pointer casts.
/// For group items (C unions), returns `&name` since unions cannot be cast
/// to pointers directly. For elementary items (arrays/scalars), returns `name`.
pub(crate) fn c_ptr_expr(c_name: &str, data_items: &[HirDataItem]) -> String {
    let lookup = extract_leaf_member(c_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) {
        if let Some((from, thru)) = &item.renames {
            if thru.is_some() {
                return c_name.to_string();
            }
            let from_c = sanitize_name(from);
            if let Some(from_item) = find_data_item_by_sanitized_name(&from_c, data_items) {
                if matches!(from_item.data_type, HirType::Group { .. }) {
                    return c_name.to_string();
                }
            }
        }
    }

    if let Some(item) = find_data_item_by_c_name(c_name, data_items) {
        match &item.data_type {
            HirType::Alphanumeric { .. } | HirType::National { .. } => c_name.to_string(),
            HirType::Group { .. }
            | HirType::Numeric { .. }
            | HirType::Comp3 { .. }
            | HirType::Binary { .. }
            | HirType::Index
            | HirType::Boolean
            | HirType::FloatShort
            | HirType::FloatLong
            | HirType::FloatExtended => format!("&{c_name}"),
            HirType::Pointer => c_name.to_string(),
        }
    } else {
        let lookup = extract_leaf_member(c_name);
        let is_display_numeric = with_active_context(|ctx| ctx.has_display_numeric(lookup));
        if is_display_numeric {
            c_name.to_string()
        } else if is_group_item_c(c_name, data_items) || is_numeric_item_c(c_name, data_items) {
            format!("&{c_name}")
        } else if c_name.contains('[') || c_name.contains(".members.") {
            format!("&({c_name})")
        } else {
            c_name.to_string()
        }
    }
}

pub(crate) fn find_original_data_item_by_sanitized_name<'a>(
    c_name: &str,
    items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    for item in items {
        if sanitize_name(&item.name) == c_name {
            return Some(item);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_original_data_item_by_sanitized_name(c_name, members) {
                return Some(found);
            }
        }
    }
    None
}

pub(crate) fn find_data_item_by_c_name<'a>(
    c_name: &str,
    data_items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    if c_name.contains("__") {
        let path: Vec<&str> = c_name
            .split("__")
            .map(|part| part.split('[').next().unwrap_or(part))
            .collect();
        let mut current_items = data_items;
        let mut found: Option<&HirDataItem> = None;
        for (idx, segment) in path.iter().enumerate() {
            let item = current_items
                .iter()
                .find(|item| sanitize_name(&item.name) == *segment)?;
            found = Some(item);
            if idx + 1 < path.len() {
                match &item.data_type {
                    HirType::Group { members, .. } => current_items = members,
                    _ => return found,
                }
            }
        }
        if found.is_some() {
            return found;
        }
    }

    let lookup = extract_leaf_member(c_name);
    for item in data_items {
        if sanitize_name(&item.name) == lookup {
            return Some(item);
        }
    }
    find_data_item_by_sanitized_name(lookup, data_items)
}

pub(crate) fn find_data_item_by_sanitized_name<'a>(
    c_name: &str,
    items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    for item in items {
        if sanitize_name(&item.name) == c_name {
            if let Some((from, _thru)) = &item.renames {
                let from_c = sanitize_name(from);
                if from_c != c_name {
                    if let Some(found) = find_data_item_by_sanitized_name(&from_c, items) {
                        return Some(found);
                    }
                }
            }
            return Some(item);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_data_item_by_sanitized_name(c_name, members) {
                return Some(found);
            }
        }
    }
    None
}

/// Resolve a variable name to its fully-qualified C name.
/// If the variable is a group member, returns the qualified path
/// (e.g., `WS_SRC.members._m_FIELD_A`).
/// If it's a top-level variable, returns `sanitize_name(name)`.
/// Get the group members of a data item by COBOL name.
pub(crate) fn get_group_members<'a>(
    name: &HirDataName,
    data_items: &'a [HirDataItem],
) -> &'a [HirDataItem] {
    if let Some(item) = find_data_item_by_name(name, data_items) {
        if let HirType::Group { members, .. } = &item.data_type {
            return members;
        }
    }
    &[]
}

fn child_data_name(parent: &HirDataName, child: &HirDataItem) -> HirDataName {
    let mut qualifiers = parent.qualifiers.clone();
    qualifiers.insert(0, parent.name.clone());
    HirDataName::new(child.name.clone(), qualifiers)
}

/// Emit MOVE CORRESPONDING: for each member name in `from` group that also
/// exists in `to` group, generate a MOVE from from.member to to.member.
pub(crate) fn emit_corresponding_move(
    out: &mut String,
    from: &HirDataName,
    to: &HirDataName,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let c_from = data_name_to_c_name(from);
    let c_to = data_name_to_c_name(to);
    out.push_str(&format!(
        "{pad}/* MOVE CORRESPONDING {c_from} TO {c_to} */\n"
    ));
    emit_corresponding_move_members(out, from, to, data_items, pad);
}

fn emit_corresponding_move_members(
    out: &mut String,
    from: &HirDataName,
    to: &HirDataName,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let from_members = get_group_members(from, data_items);
    let to_members = get_group_members(to, data_items);
    for src_item in from_members {
        for tgt_item in to_members {
            if src_item.name == tgt_item.name && src_item.name != "FILLER" && src_item.name != "PIC"
            {
                if src_item.occurs.is_some() || tgt_item.occurs.is_some() {
                    continue;
                }
                let src_q = data_name_to_c_name(&child_data_name(from, src_item));
                let tgt_q = data_name_to_c_name(&child_data_name(to, tgt_item));
                let src_ptr = c_ptr_expr(&src_q, data_items);
                let tgt_ptr = c_ptr_expr(&tgt_q, data_items);

                if matches!(src_item.data_type, HirType::Group { .. })
                    && matches!(tgt_item.data_type, HirType::Group { .. })
                    && corresponding_groups_have_common_children(src_item, tgt_item)
                {
                    emit_corresponding_move_members(
                        out,
                        &child_data_name(from, src_item),
                        &child_data_name(to, tgt_item),
                        data_items,
                        pad,
                    );
                    continue;
                }
                match (&src_item.data_type, &tgt_item.data_type) {
                    (
                        HirType::Numeric {
                            size: src_disp,
                            decimal_places: 0,
                            ..
                        },
                        HirType::Numeric {
                            size: tgt_disp,
                            decimal_places: 0,
                            ..
                        },
                    ) => {
                        // Both are display numeric in groups (char[])
                        let src_ptr = display_numeric_const_ptr(&src_q);
                        let tgt_ptr = display_numeric_ptr(&tgt_q);
                        let src_read = format!(
                            "cobol_display_to_int64(\
                             {src_ptr}, {src_disp})"
                        );
                        out.push_str(&format!(
                            "{pad}cobol_store_numeric_display({src_read}, \
                             {tgt_ptr}, {tgt_disp});\n"
                        ));
                    }
                    (HirType::Numeric { .. }, HirType::Numeric { .. })
                        if is_group_member_field(&src_q) || is_group_member_field(&tgt_q) =>
                    {
                        out.push_str(&format!(
                            "{pad}memcpy({tgt_ptr}, {src_ptr}, sizeof({tgt_q}));\n"
                        ));
                    }
                    (HirType::Binary { .. }, HirType::Binary { .. })
                    | (HirType::Comp3 { .. }, HirType::Comp3 { .. }) => {
                        out.push_str(&format!("{pad}{tgt_q} = {src_q};\n"));
                    }
                    (HirType::Numeric { .. }, HirType::Numeric { .. }) => {
                        out.push_str(&format!("{pad}{tgt_q} = {src_q};\n"));
                    }
                    (
                        HirType::Alphanumeric { size: src_sz },
                        HirType::Alphanumeric { size: tgt_sz },
                    ) => {
                        let copy_len = std::cmp::min(*src_sz, *tgt_sz);
                        out.push_str(&format!("{pad}memcpy({tgt_q}, {src_q}, {copy_len});\n"));
                        if *tgt_sz > *src_sz {
                            out.push_str(&format!(
                                "{pad}memset({tgt_q} + {src_sz}, ' ', {});\n",
                                tgt_sz - src_sz
                            ));
                        }
                    }
                    _ => {
                        out.push_str(&format!(
                            "{pad}memcpy({tgt_ptr}, {src_ptr}, sizeof({tgt_q}));\n"
                        ));
                    }
                }
            }
        }
    }
}

fn corresponding_groups_have_common_children(
    src_item: &HirDataItem,
    tgt_item: &HirDataItem,
) -> bool {
    let HirType::Group {
        members: src_members,
        ..
    } = &src_item.data_type
    else {
        return false;
    };
    let HirType::Group {
        members: tgt_members,
        ..
    } = &tgt_item.data_type
    else {
        return false;
    };
    src_members.iter().any(|src_child| {
        src_child.name != "FILLER"
            && src_child.name != "PIC"
            && src_child.occurs.is_none()
            && tgt_members.iter().any(|tgt_child| {
                tgt_child.name == src_child.name
                    && tgt_child.name != "FILLER"
                    && tgt_child.name != "PIC"
                    && tgt_child.occurs.is_none()
            })
    })
}

/// Emit ADD/SUBTRACT CORRESPONDING: for each matching numeric member,
/// generate target.member = target.member op source.member.
pub(crate) fn emit_corresponding_arith(
    out: &mut String,
    from: &HirDataName,
    to: &HirDataName,
    op: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let from_members = get_group_members(from, data_items);
    let to_members = get_group_members(to, data_items);
    let c_from = data_name_to_c_name(from);
    let c_to = data_name_to_c_name(to);
    let op_name = if op == "+" { "ADD" } else { "SUBTRACT" };
    out.push_str(&format!(
        "{pad}/* {op_name} CORRESPONDING {c_from} TO {c_to} */\n"
    ));
    for src_item in from_members {
        for tgt_item in to_members {
            if src_item.name == tgt_item.name
                && src_item.name != "FILLER"
                && src_item.name != "PIC"
                && is_numeric_type(&tgt_item.data_type)
            {
                // Use qualified member macros so nested members and REDEFINES
                // are addressed through the same path resolution as MOVE CORR.
                let src_ref = data_name_to_c_name(&child_data_name(from, src_item));
                let tgt_ref = data_name_to_c_name(&child_data_name(to, tgt_item));
                let src_value = if needs_decimal(&src_item.data_type) {
                    format!("cobol_decimal_to_int64(&{src_ref})")
                } else if let Some(src_disp_size) = grp_display_size(&src_ref, data_items) {
                    let src_ref_ptr = display_numeric_const_ptr(&src_ref);
                    format!("cobol_display_to_int64({src_ref_ptr}, {src_disp_size})")
                } else {
                    src_ref.clone()
                };
                if needs_decimal(&tgt_item.data_type) {
                    // CobolDecimal: use runtime functions
                    let func = if op == "+" {
                        "cobol_decimal_add"
                    } else {
                        "cobol_decimal_sub"
                    };
                    out.push_str(&format!(
                        "{pad}{func}(&{src_ref}, &{tgt_ref}, &{tgt_ref});\n"
                    ));
                } else if let Some(disp_size) = grp_display_size(&tgt_ref, data_items) {
                    let tgt_ref_const_ptr = display_numeric_const_ptr(&tgt_ref);
                    let tgt_ref_ptr = display_numeric_ptr(&tgt_ref);
                    out.push_str(&format!(
                        "{pad}cobol_store_numeric_display(\
                         cobol_display_to_int64(\
                         {tgt_ref_const_ptr}, {disp_size}) {op} \
                         ({src_value}), \
                         {tgt_ref_ptr}, {disp_size});\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}{tgt_ref} = {tgt_ref} {op} {src_value};\n"));
                }
            }
        }
    }
}

/// Check if a HirType is a numeric type (suitable for arithmetic CORRESPONDING).
pub(crate) fn is_numeric_type(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::Numeric { .. }
            | HirType::Binary { .. }
            | HirType::Comp3 { .. }
            | HirType::FloatShort
            | HirType::FloatLong
            | HirType::FloatExtended
    )
}

/// Get the maximum integer value for a PIC 9(N) field.
pub(crate) fn get_pic_max(name: &str, data_items: &[HirDataItem]) -> Option<i64> {
    let item = find_data_item(name, data_items)?;
    match &item.data_type {
        HirType::Numeric { size, .. } => Some(10_i64.pow(*size) - 1),
        HirType::Binary { size } => Some(10_i64.pow(*size) - 1),
        _ => None,
    }
}

/// Emit overflow check for integer (non-decimal) arithmetic targets.
/// Expects `_prev` and `_size_error` to be in scope.
pub(crate) fn emit_integer_overflow_check(
    out: &mut String,
    target_name: &str,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(max_val) = get_pic_max(target_name, data_items) {
        let c_tgt_base = sanitize_name(target_name);
        if let Some(disp_size) = grp_display_size(&c_tgt_base, data_items) {
            let c_target_const_ptr = display_numeric_const_ptr(c_target);
            let c_target_ptr = display_numeric_ptr(c_target);
            out.push_str(&format!(
                "{pad}if (llabs(cobol_display_to_int64(\
                 {c_target_const_ptr}, {disp_size})) > {max_val}) \
                 {{ _size_error = 1; cobol_store_numeric_display(\
                 _prev, {c_target_ptr}, {disp_size}); }}\n"
            ));
        } else {
            out.push_str(&format!(
                "{pad}if (llabs({c_target}) > {max_val}) \
                 {{ _size_error = 1; {c_target} = _prev; }}\n"
            ));
        }
    }
}

/// Emit COMPUTE with overflow check: save, assign, check, restore on overflow.
pub(crate) fn emit_save_and_check_overflow(
    out: &mut String,
    target_name: &str,
    c_target: &str,
    c_expr: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_is_decimal =
        find_data_item(target_name, data_items).is_some_and(|i| needs_decimal(&i.data_type));
    if target_is_decimal {
        // CobolDecimal target: save/restore struct, convert expression
        out.push_str(&format!("{pad}{{ CobolDecimal _prev = {c_target};\n"));
        out.push_str(&format!(
            "{pad}cobol_decimal_from_int((int64_t)({c_expr}), 0, &{c_target});\n"
        ));
        if let Some(max_val) = get_pic_max(target_name, data_items) {
            out.push_str(&format!(
                "{pad}if (llabs({c_target}.value) > {max_val}) \
                 {{ _size_error = 1; {c_target} = _prev; }}\n"
            ));
        }
        out.push_str(&format!("{pad}}}\n"));
    } else {
        let c_tgt_base = sanitize_name(target_name);
        if let Some(disp_size) = grp_display_size(&c_tgt_base, data_items) {
            let c_target_const_ptr = display_numeric_const_ptr(c_target);
            let c_target_ptr = display_numeric_ptr(c_target);
            out.push_str(&format!(
                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                 {c_target_const_ptr}, {disp_size});\n"
            ));
            out.push_str(&format!(
                "{pad}cobol_store_numeric_display({c_expr}, \
                 {c_target_ptr}, {disp_size});\n"
            ));
            if let Some(max_val) = get_pic_max(target_name, data_items) {
                out.push_str(&format!(
                    "{pad}if (llabs(cobol_display_to_int64(\
                     {c_target_const_ptr}, {disp_size})) > {max_val}) \
                     {{ _size_error = 1; cobol_store_numeric_display(\
                     _prev, {c_target_ptr}, {disp_size}); }}\n"
                ));
            }
            out.push_str(&format!("{pad}}}\n"));
        } else {
            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
            out.push_str(&format!("{pad}{c_target} = {c_expr};\n"));
            if let Some(max_val) = get_pic_max(target_name, data_items) {
                out.push_str(&format!(
                    "{pad}if (llabs({c_target}) > {max_val}) \
                     {{ _size_error = 1; {c_target} = _prev; }}\n"
                ));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
    }
}

/// Generate a PICTURE string for use with cobol_decimal_to_display.
/// E.g., Numeric { size: 5, decimal_places: 2, is_signed: true } => "-999.99"
pub(crate) fn generate_pic_string(data_type: &HirType) -> String {
    match data_type {
        HirType::Numeric {
            size,
            decimal_places,
            ..
        }
        | HirType::Comp3 {
            size,
            decimal_places,
        } => {
            let is_signed = match data_type {
                HirType::Numeric { is_signed, .. } => *is_signed,
                _ => true,
            };
            let int_digits = *size as usize - *decimal_places as usize;
            let mut pic = String::new();
            if is_signed {
                pic.push('-');
            }
            for _ in 0..int_digits {
                pic.push('9');
            }
            if *decimal_places > 0 {
                pic.push('.');
                for _ in 0..*decimal_places {
                    pic.push('9');
                }
            }
            pic
        }
        _ => "9".to_string(),
    }
}

/// Emit a decimal arithmetic operation.
/// Converts the operand to a CobolDecimal temporary if needed, then calls the runtime function.
pub(crate) fn emit_decimal_arith(
    out: &mut String,
    c_target: &str,
    operand: &HirExpr,
    func: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some((target_size, target_scale, target_signed)) =
        display_numeric_c_expr_metadata(c_target, data_items)
    {
        let c_target_const_ptr = display_numeric_const_ptr(c_target);
        let c_target_ptr = display_numeric_ptr(c_target);
        let operand_init = decimal_temp_init_from_expr("_rhs", operand, data_items);
        let signed = c_bool(target_signed);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _lhs; \
             cobol_decimal_from_int(cobol_display_to_int64({c_target_const_ptr}, {target_size}), {target_scale}, &_lhs); \
             _lhs.size = {target_size}; _lhs.scale = {target_scale}; _lhs.is_signed = {signed}; \
             {operand_init} \
             {func}(&_lhs, &_rhs, &_lhs); \
             {} \
             cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }}\n",
            decimal_rescale_to_scale_statement("_lhs", target_scale)
        ));
        return;
    }

    // Check if operand is already a decimal variable
    let op_is_decimal = is_decimal_expr(operand, data_items);

    if op_is_decimal {
        let c_op = emit_expr(operand);
        out.push_str(&format!(
            "{pad}{func}(&{c_target}, &{c_op}, &{c_target});\n"
        ));
    } else {
        match signed_decimal_literal_expr(operand) {
            Some((scaled, scale)) => {
                out.push_str(&format!(
                    "{pad}{{ CobolDecimal _tmp; cobol_decimal_from_int({scaled}, {scale}, &_tmp); {func}(&{c_target}, &_tmp, &{c_target}); }}\n"
                ));
            }
            None => match operand {
                HirExpr::Literal(HirLiteral::String(s)) => {
                    let escaped = escape_c_string(s);
                    let len = s.len();
                    out.push_str(&format!(
                    "{pad}{{ CobolDecimal _tmp; cobol_decimal_from_string((const uint8_t*)\"{escaped}\", {len}, &_tmp); {func}(&{c_target}, &_tmp, &{c_target}); }}\n"
                ));
                }
                _ if expr_contains_decimal(operand) => {
                    let c_op = emit_expr_as_double(operand);
                    out.push_str(&format!(
                        "{pad}{{ CobolDecimal _tmp = {{ .value = 0, .scale = 9, .size = 18, .is_signed = 1 }}; cobol_decimal_from_double({c_op}, &_tmp); {func}(&{c_target}, &_tmp, &{c_target}); }}\n"
                    ));
                }
                _ => {
                    let init = decimal_temp_init_from_expr("_tmp", operand, data_items);
                    out.push_str(&format!(
                        "{pad}{{ {init} {func}(&{c_target}, &_tmp, &{c_target}); }}\n"
                    ));
                }
            },
        }
    }
}

pub(crate) fn display_numeric_c_expr_info(
    c_expr: &str,
    data_items: &[HirDataItem],
) -> Option<(u32, u32)> {
    display_numeric_c_expr_metadata(c_expr, data_items).map(|(size, scale, _)| (size, scale))
}

fn c_bool(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

pub(crate) fn display_numeric_c_expr_metadata(
    c_expr: &str,
    data_items: &[HirDataItem],
) -> Option<(u32, u32, bool)> {
    let leaf = extract_leaf_member(c_expr);
    let item = display_numeric_c_expr_item(c_expr, data_items);
    let item_numeric = item.and_then(|item| match &item.data_type {
        HirType::Numeric {
            size,
            decimal_places,
            is_signed,
        } => Some((*size, *decimal_places, *is_signed)),
        _ => None,
    });
    let size = grp_display_size(c_expr, data_items).or_else(|| {
        with_active_context(|ctx| {
            ctx.display_numeric_size(c_expr)
                .or_else(|| ctx.display_numeric_size(leaf))
        })
    })?;
    let scale = item_numeric
        .map(|(_, scale, _)| scale)
        .or_else(|| {
            with_active_context(|ctx| {
                ctx.display_numeric_scale(c_expr)
                    .or_else(|| ctx.display_numeric_scale(leaf))
            })
        })
        .unwrap_or(0);
    let is_signed = item_numeric
        .map(|(_, _, is_signed)| is_signed)
        .unwrap_or(false);
    Some((size, scale, is_signed))
}

fn display_numeric_c_expr_item<'a>(
    c_expr: &str,
    data_items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    find_data_item_by_c_name(c_expr, data_items).or_else(|| {
        find_original_data_item_by_sanitized_name(extract_leaf_member(c_expr), data_items)
    })
}

fn decimal_temp_init_from_expr(
    temp_name: &str,
    expr: &HirExpr,
    data_items: &[HirDataItem],
) -> String {
    if let Some((scaled, scale)) = signed_decimal_literal_expr(expr) {
        return format!(
            "CobolDecimal {temp_name}; cobol_decimal_from_int({scaled}, {scale}, &{temp_name});"
        );
    }

    let c_expr = emit_expr(expr);
    if let Some((size, scale, is_signed)) = display_numeric_c_expr_metadata(&c_expr, data_items) {
        let c_ptr = display_numeric_const_ptr(&c_expr);
        let signed = c_bool(is_signed);
        return format!(
            "CobolDecimal {temp_name}; cobol_decimal_from_int(cobol_display_to_int64({c_ptr}, {size}), {scale}, &{temp_name}); {temp_name}.size = {size}; {temp_name}.scale = {scale}; {temp_name}.is_signed = {signed};"
        );
    }

    if is_decimal_expr(expr, data_items) {
        return format!("CobolDecimal {temp_name} = {c_expr};");
    }

    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            format!(
                "CobolDecimal {temp_name}; cobol_decimal_from_string((const uint8_t*)\"{escaped}\", {len}, &{temp_name});"
            )
        }
        _ if expr_contains_decimal(expr) => {
            let c_op = emit_expr_as_double(expr);
            format!(
                "CobolDecimal {temp_name} = {{ .value = 0, .scale = 9, .size = 18, .is_signed = 1 }}; cobol_decimal_from_double({c_op}, &{temp_name});"
            )
        }
        _ => {
            let c_op = emit_int_compatible_expr(expr, data_items);
            format!("CobolDecimal {temp_name}; cobol_decimal_from_int({c_op}, 0, &{temp_name});")
        }
    }
}

pub(crate) fn expr_is_scaled_display_numeric(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    let c_expr = emit_expr(expr);
    display_numeric_c_expr_metadata(&c_expr, data_items).is_some_and(|(_, scale, _)| scale > 0)
}

fn decimal_add_exact_statement(acc: &str, rhs: &str) -> String {
    format!(
        "{{ int32_t _scale = {acc}.scale > {rhs}.scale ? {acc}.scale : {rhs}.scale; \
           __int128 _av = {acc}.value; \
           for (int32_t _i = 0; _i < _scale - {acc}.scale; _i++) _av *= 10; \
           __int128 _bv = {rhs}.value; \
           for (int32_t _i = 0; _i < _scale - {rhs}.scale; _i++) _bv *= 10; \
           {acc}.value = (int64_t)(_av + _bv); \
           {acc}.scale = _scale; \
           {acc}.size = {acc}.size > {rhs}.size ? {acc}.size : {rhs}.size; \
           {acc}.is_signed = {acc}.is_signed || {rhs}.is_signed; }} "
    )
}

fn decimal_rescale_to_scale_statement(source: &str, target_scale: u32) -> String {
    format!(
        "int64_t _result = {source}.value; \
         if ({source}.scale > {target_scale}) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {source}.scale - {target_scale}; _i++) _factor *= 10; \
             _result = {source}.value / _factor; \
         }} else if ({source}.scale < {target_scale}) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {target_scale} - {source}.scale; _i++) _factor *= 10; \
             _result = {source}.value * _factor; \
         }} "
    )
}

/// Emit ADD GIVING for decimal: add all operands and TO values, store in GIVING target.
pub(crate) fn emit_decimal_giving_add(
    out: &mut String,
    operands: &[HirExpr],
    to: &[HirExpr],
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some((target_size, target_scale, _)) =
        display_numeric_c_expr_metadata(c_target, data_items)
    {
        let terms: Vec<&HirExpr> = operands.iter().chain(to.iter()).collect();
        let Some((first_term, rest_terms)) = terms.split_first() else {
            return;
        };
        let init = decimal_temp_init_from_expr("_sum", first_term, data_items);
        out.push_str(&format!("{pad}{{ {init} "));
        out.push_str("_sum.size = 18; _sum.is_signed = 1; ");
        for (idx, term) in rest_terms.iter().enumerate() {
            let rhs = format!("_rhs{idx}");
            out.push_str(&decimal_temp_init_from_expr(&rhs, term, data_items));
            out.push_str(&decimal_add_exact_statement("_sum", &rhs));
        }
        let c_target_ptr = display_numeric_ptr(c_target);
        out.push_str(&decimal_rescale_to_scale_statement("_sum", target_scale));
        out.push_str(&format!(
            "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }}\n"
        ));
        return;
    }

    let terms: Vec<&HirExpr> = operands.iter().chain(to.iter()).collect();
    let Some((first_term, rest_terms)) = terms.split_first() else {
        return;
    };
    let init = decimal_temp_init_from_expr("_sum", first_term, data_items);
    out.push_str(&format!("{pad}{{ {init} "));
    out.push_str("_sum.size = 18; _sum.is_signed = 1; ");
    for (idx, term) in rest_terms.iter().enumerate() {
        let rhs = format!("_rhs{idx}");
        out.push_str(&decimal_temp_init_from_expr(&rhs, term, data_items));
        out.push_str(&decimal_add_exact_statement("_sum", &rhs));
    }
    if let Some(item) = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
    {
        match &item.data_type {
            HirType::Numeric {
                size,
                decimal_places,
                is_signed,
            } => {
                out.push_str(&format!(
                    "if (_sum.scale < {decimal_places}) {{ for (int32_t _i = 0; _i < {decimal_places} - _sum.scale; _i++) _sum.value *= 10; }} else if (_sum.scale > {decimal_places}) {{ for (int32_t _i = 0; _i < _sum.scale - {decimal_places}; _i++) _sum.value /= 10; }} "
                ));
                out.push_str(&format!("{c_target} = _sum; "));
                out.push_str(&format!(
                    "{c_target}.size = {size}; {c_target}.scale = {decimal_places}; {c_target}.is_signed = {}; ",
                    i32::from(*is_signed)
                ));
            }
            HirType::Comp3 {
                size,
                decimal_places,
            } => {
                out.push_str(&format!(
                    "if (_sum.scale < {decimal_places}) {{ for (int32_t _i = 0; _i < {decimal_places} - _sum.scale; _i++) _sum.value *= 10; }} else if (_sum.scale > {decimal_places}) {{ for (int32_t _i = 0; _i < _sum.scale - {decimal_places}; _i++) _sum.value /= 10; }} "
                ));
                out.push_str(&format!("{c_target} = _sum; "));
                out.push_str(&format!(
                    "{c_target}.size = {size}; {c_target}.scale = {decimal_places}; {c_target}.is_signed = 1; "
                ));
            }
            _ => {
                out.push_str(&format!("{c_target} = _sum; "));
            }
        }
    } else {
        out.push_str(&format!("{c_target} = _sum; "));
    }
    out.push_str("}\n");
}

/// Parse a decimal literal string like "123.45" into (scaled_value, scale).
/// E.g., "123.45" -> (12345, 2), "10.5" -> (105, 1), "100" -> (100, 0).
pub(crate) fn parse_decimal_literal(s: &str) -> (i64, u32) {
    let negative = s.starts_with('-');
    let body = s.trim_start_matches(['+', '-']);
    if let Some(dot_pos) = body.find('.') {
        let int_part = &body[..dot_pos];
        let frac_part = &body[dot_pos + 1..];
        let scale = frac_part.len() as u32;
        let combined: String = int_part.chars().chain(frac_part.chars()).collect();
        let abs_value: i64 = combined.parse().unwrap_or(0);
        if negative {
            (-abs_value, scale)
        } else {
            (abs_value, scale)
        }
    } else {
        let abs_value: i64 = body.parse().unwrap_or(0);
        if negative {
            (-abs_value, 0)
        } else {
            (abs_value, 0)
        }
    }
}

pub(crate) fn scale_decimal_to_target(scaled: i64, scale: u32, target_scale: u32) -> i64 {
    match target_scale.cmp(&scale) {
        std::cmp::Ordering::Greater => {
            scaled.saturating_mul(10_i64.saturating_pow(target_scale - scale))
        }
        std::cmp::Ordering::Less => scaled / 10_i64.saturating_pow(scale - target_scale),
        std::cmp::Ordering::Equal => scaled,
    }
}

fn signed_decimal_literal_expr(expr: &HirExpr) -> Option<(i64, u32)> {
    match expr {
        HirExpr::Literal(HirLiteral::Decimal(d)) => Some(parse_decimal_literal(d)),
        HirExpr::UnaryOp {
            op: HirUnaryOp::Neg,
            operand,
        } => signed_decimal_literal_expr(operand).map(|(scaled, scale)| (-scaled, scale)),
        _ => None,
    }
}

/// Emit INITIALIZE for a single field, choosing the correct default by type.
///
/// COBOL rules: ALPHANUMERIC → spaces, NUMERIC → zero, GROUP → recurse members.
pub(crate) fn emit_initialize_field(
    out: &mut String,
    name: &smol_str::SmolStr,
    c_name: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(item) = find_data_item(name.as_str(), data_items) {
        if item.occurs.is_some() && !matches!(item.data_type, HirType::Group { .. }) {
            match &item.data_type {
                HirType::Alphanumeric { size } => {
                    out.push_str(&format!(
                        "{pad}for (size_t _i = 0; _i < {}; _i++) {{ memset({c_name}[_i], ' ', {size}); }} /* INITIALIZE */\n",
                        item.occurs.unwrap_or(0)
                    ));
                }
                HirType::National { size } => {
                    out.push_str(&format!(
                        "{pad}for (size_t _i = 0; _i < {}; _i++) {{ for (uint32_t _j = 0; _j < {size}; _j++) {{ {c_name}[_i][_j] = 0x0020; }} }} /* INITIALIZE */\n",
                        item.occurs.unwrap_or(0)
                    ));
                }
                dt if needs_decimal(dt) => {
                    out.push_str(&format!(
                        "{pad}memset({c_name}, 0, sizeof({c_name})); /* INITIALIZE */\n"
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "{pad}memset({c_name}, 0, sizeof({c_name})); /* INITIALIZE */\n"
                    ));
                }
            }
            return;
        }
        match &item.data_type {
            HirType::Alphanumeric { size } => {
                let is_grp_member = with_active_context(|ctx| {
                    ctx.is_group_alpha_name(&sanitize_name(name.as_str()))
                });
                if is_grp_member {
                    out.push_str(&format!(
                        "{pad}memset({c_name}, ' ', {size}); /* INITIALIZE */\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "{pad}memset({c_name}, ' ', {size}); {c_name}[{size}] = '\\0'; /* INITIALIZE */\n"
                    ));
                }
            }
            HirType::Group { members, .. } => {
                out.push_str(&format!("{pad}/* INITIALIZE group {c_name} */\n"));
                for member in members {
                    let member_c = sanitize_name(&member.name);
                    emit_initialize_field(out, &member.name, &member_c, data_items, pad);
                }
            }
            dt if needs_decimal(dt) => {
                // CobolDecimal → zero via runtime
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int(0, 0, &{c_name}); /* INITIALIZE */\n"
                ));
            }
            _ => {
                // Numeric, Binary, Index, etc. → zero
                let base = c_name
                    .split('[')
                    .next()
                    .and_then(|s| s.split('.').next_back())
                    .map(|s| s.trim_start_matches("_m_"))
                    .unwrap_or(c_name);
                if let Some(disp_size) = grp_display_size(base, data_items) {
                    let c_name_ptr = display_numeric_ptr(c_name);
                    out.push_str(&format!(
                        "{pad}cobol_store_numeric_display(0, \
                         {c_name_ptr}, {disp_size}); /* INITIALIZE */\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}{c_name} = 0; /* INITIALIZE */\n"));
                }
            }
        }
    } else {
        // Unknown field, default to zero
        let base = c_name
            .split('[')
            .next()
            .and_then(|s| s.split('.').next_back())
            .map(|s| s.trim_start_matches("_m_"))
            .unwrap_or(c_name);
        if let Some(disp_size) = grp_display_size(base, data_items) {
            let c_name_ptr = display_numeric_ptr(c_name);
            out.push_str(&format!(
                "{pad}cobol_store_numeric_display(0, \
                 {c_name_ptr}, {disp_size}); /* INITIALIZE */\n"
            ));
        } else {
            out.push_str(&format!("{pad}{c_name} = 0; /* INITIALIZE */\n"));
        }
    }
}

/// Emit an INSPECT operand (pattern string) as a C pointer+length pair.
/// Returns (ptr_expr, len_expr) for use in runtime calls.
pub(crate) fn emit_inspect_operand(
    _out: &mut str,
    expr: &HirExpr,
    _label: &str,
    data_items: &[HirDataItem],
    _pad: &str,
) -> (String, String) {
    match expr {
        HirExpr::DataRef(data_ref) => {
            let c_name = data_name_to_c_name(&data_ref.name);
            let size = find_data_item_size(&c_name, data_items);
            let ptr = c_ptr_expr(&c_name, data_items);
            (format!("(const uint8_t*){ptr}"), format!("{size}"))
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            (format!("(const uint8_t*)\"{escaped}\""), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Space) => {
            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            let size = find_data_item_size(&c_name, data_items);
            let ptr = c_ptr_expr(&c_name, data_items);
            (format!("(const uint8_t*){ptr}"), format!("{size}"))
        }
        _ => ("NULL".to_string(), "0".to_string()),
    }
}

/// Emit INSPECT TALLYING phrases.
pub(crate) fn emit_inspect_tallying(
    out: &mut String,
    c_target: &str,
    target_size: u32,
    tallying: &[cobol_hir::HirInspectTallying],
    data_items: &[HirDataItem],
    pad: &str,
) {
    let tgt_ptr = c_ptr_expr(c_target, data_items);
    if tallying.is_empty() {
        // Fallback: count all characters
        out.push_str(&format!(
            "{pad}cobol_inspect_tallying((const uint8_t*){tgt_ptr}, {target_size}, NULL, 0, 0);\n"
        ));
        return;
    }
    for (i, t) in tallying.iter().enumerate() {
        let counter = emit_expr(&t.counter);
        let counter_base = expr_data_name(&t.counter).map(data_name_to_c_name);
        let counter_disp = counter_base
            .as_deref()
            .and_then(|name| grp_display_size(name, data_items));
        let (mode, search_ptr, search_len) = match &t.kind {
            cobol_hir::HirTallyingKind::Characters => (0u32, "NULL".to_string(), "0".to_string()),
            cobol_hir::HirTallyingKind::All(expr) => {
                let label = format!("tally_s{i}");
                let (ptr, len) = emit_inspect_operand(out, expr, &label, data_items, pad);
                (1, ptr, len)
            }
            cobol_hir::HirTallyingKind::Leading(expr) => {
                let label = format!("tally_s{i}");
                let (ptr, len) = emit_inspect_operand(out, expr, &label, data_items, pad);
                (2, ptr, len)
            }
            cobol_hir::HirTallyingKind::Trailing(expr) => {
                let label = format!("tally_s{i}");
                let (ptr, len) = emit_inspect_operand(out, expr, &label, data_items, pad);
                (3, ptr, len)
            }
        };
        if let Some(disp_size) = counter_disp {
            let counter_const_ptr = display_numeric_const_ptr(&counter);
            let counter_ptr = display_numeric_ptr(&counter);
            out.push_str(&format!(
                "{pad}cobol_store_numeric_display(\
                 cobol_display_to_int64({counter_const_ptr}, {disp_size}) + \
                 cobol_inspect_tallying((const uint8_t*){tgt_ptr}, {target_size}, \
                 {search_ptr}, {search_len}, {mode}), \
                 {counter_ptr}, {disp_size});\n"
            ));
        } else {
            out.push_str(&format!(
                "{pad}{counter} += cobol_inspect_tallying((const uint8_t*){tgt_ptr}, {target_size}, {search_ptr}, {search_len}, {mode});\n"
            ));
        }
    }
}

/// Emit INSPECT REPLACING phrases.
pub(crate) fn emit_inspect_replacing(
    out: &mut String,
    c_target: &str,
    target_size: u32,
    replacing: &[cobol_hir::HirInspectReplacing],
    data_items: &[HirDataItem],
    pad: &str,
) {
    let tgt_ptr = c_ptr_expr(c_target, data_items);
    if replacing.is_empty() {
        // Fallback: replace all characters with space
        out.push_str(&format!(
            "{pad}cobol_inspect_replacing((uint8_t*){tgt_ptr}, {target_size}, NULL, 0, (const uint8_t*)\" \", 1, 0);\n"
        ));
        return;
    }
    for (i, r) in replacing.iter().enumerate() {
        if let cobol_hir::HirReplacingKind::Characters(to_expr) = &r.kind {
            if r.before_after.len() == 1 {
                let label = format!("rep_to{i}");
                let marker_label = format!("rep_marker{i}");
                let (to_ptr, to_len) = emit_inspect_operand(out, to_expr, &label, data_items, pad);
                let (marker_ptr, marker_len) = emit_inspect_operand(
                    out,
                    &r.before_after[0].value,
                    &marker_label,
                    data_items,
                    pad,
                );
                let replace_before = r.before_after[0].is_before;
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!(
                    "{pad}    uint8_t* _insp_base = (uint8_t*){tgt_ptr};\n"
                ));
                out.push_str(&format!("{pad}    uint32_t _insp_len = {target_size};\n"));
                out.push_str(&format!(
                    "{pad}    const uint8_t* _insp_marker = {marker_ptr};\n"
                ));
                out.push_str(&format!(
                    "{pad}    uint32_t _insp_marker_len = {marker_len};\n"
                ));
                out.push_str(&format!("{pad}    uint32_t _insp_pos = _insp_len;\n"));
                out.push_str(&format!(
                    "{pad}    if (_insp_marker_len > 0 && _insp_marker_len <= _insp_len) {{\n"
                ));
                out.push_str(&format!("{pad}        for (uint32_t _i = 0; _i + _insp_marker_len <= _insp_len; _i++) {{\n"));
                out.push_str(&format!("{pad}            if (memcmp(_insp_base + _i, _insp_marker, _insp_marker_len) == 0) {{ _insp_pos = _i; break; }}\n"));
                out.push_str(&format!("{pad}        }}\n"));
                out.push_str(&format!("{pad}    }}\n"));
                if replace_before {
                    out.push_str(&format!(
                        "{pad}    cobol_inspect_replacing(_insp_base, _insp_pos, NULL, 0, {to_ptr}, {to_len}, 0);\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}    if (_insp_pos < _insp_len) {{\n"));
                    out.push_str(&format!(
                        "{pad}        uint32_t _insp_start = _insp_pos + _insp_marker_len;\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        cobol_inspect_replacing(_insp_base + _insp_start, _insp_len - _insp_start, NULL, 0, {to_ptr}, {to_len}, 0);\n"
                    ));
                    out.push_str(&format!("{pad}    }}\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
                continue;
            }
        }
        let (mode, search_ptr, search_len, replace_ptr, replace_len) = match &r.kind {
            cobol_hir::HirReplacingKind::Characters(to_expr) => {
                let label = format!("rep_to{i}");
                let (to_ptr, to_len) = emit_inspect_operand(out, to_expr, &label, data_items, pad);
                (0u32, "NULL".to_string(), "0".to_string(), to_ptr, to_len)
            }
            cobol_hir::HirReplacingKind::All { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (1, f_ptr, f_len, t_ptr, t_len)
            }
            cobol_hir::HirReplacingKind::Leading { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (2, f_ptr, f_len, t_ptr, t_len)
            }
            cobol_hir::HirReplacingKind::First { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (3, f_ptr, f_len, t_ptr, t_len)
            }
        };
        out.push_str(&format!(
            "{pad}cobol_inspect_replacing((uint8_t*){tgt_ptr}, {target_size}, {search_ptr}, {search_len}, {replace_ptr}, {replace_len}, {mode});\n"
        ));
    }
}

/// Emit the value part of a STRING source operand.
pub(crate) fn emit_string_source_value(
    out: &mut String,
    value: &HirExpr,
    i: usize,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match value {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = \"{escaped}\"; uint32_t _src_len_{i} = {len};\n"
            ));
        }
        HirExpr::DataRef(data_ref) => {
            let c_var = data_name_to_c_name(&data_ref.name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = {ptr}; uint32_t _src_len_{i} = {var_size};\n"
            ));
        }
        HirExpr::Variable(name) => {
            let c_var = data_name_to_c_name(name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = {ptr}; uint32_t _src_len_{i} = {var_size};\n"
            ));
        }
        _ => {
            let e = emit_expr(value);
            out.push_str(&format!("{pad}    int64_t _src_tmp_{i} = {e};\n"));
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = &_src_tmp_{i}; uint32_t _src_len_{i} = sizeof(int64_t);\n"
            ));
        }
    }
}

/// Emit the delimiter part of a STRING source operand.
pub(crate) fn emit_string_source_delimiter(
    out: &mut String,
    delimiter: &Option<HirExpr>,
    i: usize,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match delimiter {
        Some(HirExpr::Literal(HirLiteral::String(s))) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = (const uint8_t*)\"{escaped}\"; uint32_t _delim_len_{i} = {len};\n"
            ));
        }
        Some(HirExpr::DataRef(data_ref)) => {
            let c_var = data_name_to_c_name(&data_ref.name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = (const uint8_t*){ptr}; uint32_t _delim_len_{i} = {var_size};\n"
            ));
        }
        Some(HirExpr::Variable(name)) => {
            let c_var = data_name_to_c_name(name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = (const uint8_t*){ptr}; uint32_t _delim_len_{i} = {var_size};\n"
            ));
        }
        _ => {
            // DELIMITED BY SIZE (no delimiter)
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = NULL; uint32_t _delim_len_{i} = 0;\n"
            ));
        }
    }
}

/// Check whether an HIR expression refers to an alphanumeric field or string literal.
pub(crate) fn is_alphanumeric_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => {
            if let Some(item) = find_data_item(&data_ref.name, data_items) {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. }
                )
            } else {
                false
            }
        }
        HirExpr::Variable(name) => {
            if let Some(item) = find_data_item(name, data_items) {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. }
                )
            } else {
                false
            }
        }
        HirExpr::Subscript { variable, .. } => {
            if let Some(item) = find_data_item(variable, data_items) {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. }
                )
            } else {
                false
            }
        }
        HirExpr::Literal(HirLiteral::String(_))
        | HirExpr::Literal(HirLiteral::Space)
        | HirExpr::Literal(HirLiteral::HighValue)
        | HirExpr::Literal(HirLiteral::LowValue)
        | HirExpr::Literal(HirLiteral::Quote) => true,
        HirExpr::ReferenceModification { variable, .. } => {
            if let Some(item) = find_data_item(variable, data_items) {
                matches!(item.data_type, HirType::Alphanumeric { .. })
            } else {
                false
            }
        }
        HirExpr::FunctionCall { name, .. } => {
            let upper_fn = name.to_uppercase();
            matches!(
                upper_fn.as_str(),
                "CHAR" | "CURRENT-DATE" | "WHEN-COMPILED" | "UPPER-CASE" | "LOWER-CASE" | "REVERSE"
            )
        }
        _ => false,
    }
}

pub(crate) fn alphanumeric_expr_len(expr: &HirExpr, data_items: &[HirDataItem]) -> Option<u32> {
    match expr {
        HirExpr::DataRef(data_ref) => find_data_item(&data_ref.name, data_items).and_then(|item| {
            matches!(
                item.data_type,
                HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
            )
            .then(|| {
                let c_name = data_name_to_c_name(&data_ref.name);
                let full_size = find_data_item_size(&c_name, data_items);
                if let Some(refmod) = &data_ref.refmod {
                    if let Some(length) = &refmod.length {
                        if let HirExpr::Literal(HirLiteral::Integer(n)) = length.as_ref() {
                            (*n).max(0) as u32
                        } else {
                            full_size
                        }
                    } else if let HirExpr::Literal(HirLiteral::Integer(start)) =
                        refmod.start.as_ref()
                    {
                        full_size.saturating_sub((*start).saturating_sub(1) as u32)
                    } else {
                        full_size
                    }
                } else {
                    full_size
                }
            })
        }),
        HirExpr::Variable(name) => find_data_item(name, data_items).and_then(|item| {
            matches!(
                item.data_type,
                HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
            )
            .then(|| {
                let c_name = data_name_to_c_name(name);
                find_data_item_size(&c_name, data_items)
            })
        }),
        HirExpr::Subscript { variable, .. } => {
            find_data_item(variable, data_items).and_then(|item| {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
                )
                .then(|| {
                    let c_name = data_name_to_c_name(variable);
                    find_data_item_size(&c_name, data_items)
                })
            })
        }
        HirExpr::ReferenceModification {
            length, variable, ..
        } => {
            if let Some(len) = length {
                if let HirExpr::Literal(HirLiteral::Integer(n)) = len.as_ref() {
                    Some((*n).max(0) as u32)
                } else {
                    let c_name = data_name_to_c_name(variable);
                    Some(find_data_item_size(&c_name, data_items))
                }
            } else {
                let c_name = data_name_to_c_name(variable);
                Some(find_data_item_size(&c_name, data_items))
            }
        }
        HirExpr::Literal(HirLiteral::String(s)) => Some(s.len() as u32),
        HirExpr::Literal(HirLiteral::Space)
        | HirExpr::Literal(HirLiteral::HighValue)
        | HirExpr::Literal(HirLiteral::LowValue)
        | HirExpr::Literal(HirLiteral::Quote)
        | HirExpr::Literal(HirLiteral::Zero) => Some(1),
        HirExpr::Literal(HirLiteral::Integer(n)) => Some(n.to_string().len() as u32),
        HirExpr::Literal(HirLiteral::Decimal(d)) => Some(d.len() as u32),
        _ => None,
    }
}

fn padded_numeric_literal_for_alphanumeric(expr: &HirExpr, width: u32) -> Option<String> {
    let width = width as usize;
    match expr {
        HirExpr::Literal(HirLiteral::Zero) => Some(format!("{:0width$}", 0, width = width.max(1))),
        HirExpr::Literal(HirLiteral::Integer(n)) if *n >= 0 => {
            Some(format!("{:0width$}", n, width = width.max(1)))
        }
        HirExpr::Literal(HirLiteral::Space) => Some(" ".repeat(width)),
        HirExpr::Literal(HirLiteral::HighValue) => Some("\\xFF".repeat(width)),
        HirExpr::Literal(HirLiteral::LowValue) => Some("\\x00".repeat(width)),
        HirExpr::Literal(HirLiteral::Quote) => Some("\\\"".repeat(width)),
        _ => None,
    }
}

/// Produce `(ptr_expr, len_expr)` for an alphanumeric comparison operand.
pub(crate) fn emit_alphanumeric_operand(
    expr: &HirExpr,
    data_items: &[HirDataItem],
) -> (String, String) {
    match expr {
        HirExpr::DataRef(data_ref) => {
            let c_name = emit_data_ref_expr(data_ref);
            let base_name = data_name_to_c_name(&data_ref.name);
            let full_size = find_data_item_size(&base_name, data_items);
            if data_ref.refmod.is_none() {
                if let Some(item) = find_data_item_by_name(&data_ref.name, data_items) {
                    if let Some(operand) = numeric_display_alphanumeric_operand(
                        expr, &c_name, item, full_size, data_items,
                    ) {
                        return operand;
                    }
                }
            }
            let size = if let Some(refmod) = &data_ref.refmod {
                if let Some(length) = &refmod.length {
                    emit_expr_as_numeric(length)
                } else {
                    let start = emit_expr_as_numeric(&refmod.start);
                    format!("(({full_size}) - ({start}) + 1)")
                }
            } else {
                format!("{full_size}")
            };
            let ptr = alphanumeric_operand_ptr_expr(expr, &c_name, data_items);
            (ptr, size)
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            let size = find_data_item_size(&c_name, data_items);
            if let Some(item) = find_data_item_by_name(name, data_items) {
                if let Some(operand) =
                    numeric_display_alphanumeric_operand(expr, &c_name, item, size, data_items)
                {
                    return operand;
                }
            }
            let ptr = alphanumeric_operand_ptr_expr(expr, &c_name, data_items);
            (ptr, format!("{size}"))
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            (format!("(const uint8_t*)\"{}\"", escaped), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Space) => {
            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            ("(const uint8_t*)\"\\xFF\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            ("(const uint8_t*)\"\\x00\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            ("(const uint8_t*)\"\\\"\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            // Numeric literal compared with alphanumeric: convert to string
            let s = n.to_string();
            let len = s.len();
            (format!("(const uint8_t*)\"{}\"", s), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let len = d.len();
            (format!("(const uint8_t*)\"{}\"", d), format!("{len}"))
        }
        HirExpr::Subscript { .. } => {
            let c_name = emit_expr(expr);
            let size = if let HirExpr::Subscript { variable, .. } = expr {
                let sz = find_data_item_size(&data_name_to_c_name(variable), data_items);
                format!("{sz}")
            } else {
                format!("sizeof({c_name})")
            };
            if let Some(name) = expr_data_name(expr) {
                if let Some(item) = find_data_item_by_name(name, data_items) {
                    if let Some(operand) = numeric_display_alphanumeric_operand(
                        expr,
                        &c_name,
                        item,
                        size.parse().unwrap_or(64),
                        data_items,
                    ) {
                        return operand;
                    }
                }
            }
            let ptr = alphanumeric_operand_ptr_expr(expr, &c_name, data_items);
            (ptr, size)
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_fn = name.to_uppercase();
            match upper_fn.as_str() {
                "CHAR" => {
                    // Returns a 1-byte buffer pointer
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*){e}"), "1".to_string())
                }
                "CURRENT-DATE" | "WHEN-COMPILED" => {
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*){e}"), "21".to_string())
                }
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    let size: u32 = if let Some(arg) = args.first() {
                        if let HirExpr::DataRef(data_ref) = arg {
                            find_data_item_size(&data_name_to_c_name(&data_ref.name), data_items)
                        } else if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&data_name_to_c_name(v), data_items)
                        } else if let HirExpr::Literal(HirLiteral::String(s)) = arg {
                            s.len() as u32
                        } else {
                            64
                        }
                    } else {
                        64
                    };
                    let func = match upper_fn.as_str() {
                        "UPPER-CASE" => "cobol_func_upper_case",
                        "LOWER-CASE" => "cobol_func_lower_case",
                        _ => "cobol_func_reverse",
                    };
                    let c_arg = if let Some(arg) = args.first() {
                        emit_expr(arg)
                    } else {
                        "\"\"".to_string()
                    };
                    (
                        format!(
                            "({{ static uint8_t _fbuf[{size}]; \
                             memcpy(_fbuf, (const uint8_t*){c_arg}, {size}); \
                             {func}(_fbuf, {size}); \
                             (const uint8_t*)_fbuf; }})"
                        ),
                        format!("{size}"),
                    )
                }
                _ => {
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*)&{e}"), format!("sizeof({e})"))
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } => {
            let c_src = data_name_to_c_name(variable);
            let src_ptr = c_ptr_expr(&c_src, data_items);
            let c_start = emit_expr(start);
            let src_full_size = find_data_item_size(&c_src, data_items);
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({src_full_size} - ({c_start} - 1))")
            };
            (
                format!("(const uint8_t*){src_ptr} + ({c_start} - 1)"),
                c_len,
            )
        }
        _ => {
            // Fallback for non-alphanumeric expressions used in mixed comparisons.
            // Use a statement expression to create a temporary so we can take its address
            // (rvalues like `(int64_t)'B'` cannot have their address taken directly).
            let e = emit_expr(expr);
            (
                format!("({{ int64_t _cmp_tmp = {e}; (const uint8_t*)&_cmp_tmp; }})"),
                "sizeof(int64_t)".to_string(),
            )
        }
    }
}

fn numeric_display_alphanumeric_operand(
    expr: &HirExpr,
    c_expr: &str,
    item: &HirDataItem,
    size: u32,
    data_items: &[HirDataItem],
) -> Option<(String, String)> {
    match &item.data_type {
        HirType::Numeric {
            decimal_places: 0, ..
        } => {
            if grp_display_size(c_expr, data_items).is_some() || is_group_member_field(c_expr) {
                Some((display_numeric_const_ptr(c_expr), format!("{size}")))
            } else {
                let value = emit_int_compatible_expr(expr, data_items);
                let blank_when_zero = if item.blank_when_zero {
                    format!("if ({value} == 0) {{ memset(_cmp_num_buf, ' ', {size}); }} else ")
                } else {
                    String::new()
                };
                let pic = item
                    .picture
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| generate_pic_string(&item.data_type));
                let escaped_pic = escape_c_string(&pic);
                let pic_len = pic.len();
                Some((
                    format!(
                        "({{ static uint8_t _cmp_num_buf[64]; \
                         {blank_when_zero}{{ CobolDecimal _cmp_dec = {{ .value = ({value}), .scale = 0, .size = {size}, .is_signed = 0 }}; \
                         cobol_decimal_to_display(&_cmp_dec, _cmp_num_buf, 64, \
                         (const uint8_t*)\"{escaped_pic}\", {pic_len}); }} \
                         (const uint8_t*)_cmp_num_buf; }})"
                    ),
                    format!("{size}"),
                ))
            }
        }
        HirType::Binary { .. } | HirType::Index => {
            let value = emit_int_compatible_expr(expr, data_items);
            Some((
                format!(
                    "({{ static uint8_t _cmp_num_buf[64]; \
                     cobol_move_numeric_to_display({value}, 0, _cmp_num_buf, {size}); \
                     (const uint8_t*)_cmp_num_buf; }})"
                ),
                format!("{size}"),
            ))
        }
        ty if needs_decimal(ty) => {
            let pic_str = item
                .picture
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| generate_pic_string(ty));
            let escaped_pic = escape_c_string(&pic_str);
            let pic_len = pic_str.len();
            Some((
                format!(
                    "({{ static uint8_t _cmp_dec_buf[64]; \
                     cobol_decimal_to_display(&{c_expr}, _cmp_dec_buf, 64, \
                     (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
                     (const uint8_t*)_cmp_dec_buf; }})"
                ),
                format!("{size}"),
            ))
        }
        _ => None,
    }
}

pub(crate) fn emit_condition(cond: &HirCondition, data_items: &[HirDataItem]) -> String {
    with_active_context(|ctx| emit_condition_with_ctx(cond, data_items, ctx))
}

pub(crate) fn emit_condition_with_ctx(
    cond: &HirCondition,
    data_items: &[HirDataItem],
    ctx: &CodegenContext,
) -> String {
    let emit_expr_as_double = |expr| super::emit_expr_as_double_with_ctx(expr, ctx);
    match cond {
        HirCondition::Compare { left, op, right } => {
            if is_alphanumeric_expr(left, data_items) || is_alphanumeric_expr(right, data_items) {
                // Alphanumeric comparison via runtime function
                let (a_ptr, a_len) = if let Some(width) = alphanumeric_expr_len(right, data_items) {
                    if let Some(s) = padded_numeric_literal_for_alphanumeric(left, width) {
                        (format!("(const uint8_t*)\"{s}\""), format!("{width}"))
                    } else {
                        emit_alphanumeric_operand(left, data_items)
                    }
                } else {
                    emit_alphanumeric_operand(left, data_items)
                };
                let (b_ptr, b_len) = if let Some(width) = alphanumeric_expr_len(left, data_items) {
                    if let Some(s) = padded_numeric_literal_for_alphanumeric(right, width) {
                        (format!("(const uint8_t*)\"{s}\""), format!("{width}"))
                    } else {
                        emit_alphanumeric_operand(right, data_items)
                    }
                } else {
                    emit_alphanumeric_operand(right, data_items)
                };
                let cmp = format!("cobol_compare_alphanumeric({a_ptr}, {a_len}, {b_ptr}, {b_len})");
                let op_str = match op {
                    HirCompareOp::Eq => "== 0",
                    HirCompareOp::Ne => "!= 0",
                    HirCompareOp::Gt => "> 0",
                    HirCompareOp::Lt => "< 0",
                    HirCompareOp::Ge => ">= 0",
                    HirCompareOp::Le => "<= 0",
                };
                format!("({cmp} {op_str})")
            } else if is_decimal_expr(left, data_items)
                || is_decimal_expr(right, data_items)
                || expr_is_scaled_display_numeric(left, data_items)
                || expr_is_scaled_display_numeric(right, data_items)
            {
                // CobolDecimal comparison via runtime function
                let op_str = match op {
                    HirCompareOp::Eq => "== 0",
                    HirCompareOp::Ne => "!= 0",
                    HirCompareOp::Gt => "> 0",
                    HirCompareOp::Lt => "< 0",
                    HirCompareOp::Ge => ">= 0",
                    HirCompareOp::Le => "<= 0",
                };
                let left_init = decimal_temp_init_from_expr("_lcmp", left, data_items);
                let right_init = decimal_temp_init_from_expr("_rcmp", right, data_items);
                format!(
                    "(({{ {left_init} {right_init} \
                     cobol_decimal_cmp(&_lcmp, &_rcmp); }}) {op_str})"
                )
            } else if expr_contains_decimal(left) || expr_contains_decimal(right) {
                let l = emit_expr_as_double(left);
                let r = emit_expr_as_double(right);
                let op_str = match op {
                    HirCompareOp::Eq => "==",
                    HirCompareOp::Ne => "!=",
                    HirCompareOp::Gt => ">",
                    HirCompareOp::Lt => "<",
                    HirCompareOp::Ge => ">=",
                    HirCompareOp::Le => "<=",
                };
                format!("({l} {op_str} {r})")
            } else {
                let l = emit_int_compatible_expr(left, data_items);
                let r = emit_int_compatible_expr(right, data_items);
                let op_str = match op {
                    HirCompareOp::Eq => "==",
                    HirCompareOp::Ne => "!=",
                    HirCompareOp::Gt => ">",
                    HirCompareOp::Lt => "<",
                    HirCompareOp::Ge => ">=",
                    HirCompareOp::Le => "<=",
                };
                format!("{l} {op_str} {r}")
            }
        }
        HirCondition::ClassCondition { operand, class } => {
            let (ptr, len) = emit_alphanumeric_operand(operand, data_items);
            let func = match class {
                HirClassType::Numeric => "cobol_is_numeric",
                HirClassType::Alphabetic => "cobol_is_alphabetic",
                HirClassType::AlphabeticLower => "cobol_is_alphabetic_lower",
                HirClassType::AlphabeticUpper => "cobol_is_alphabetic_upper",
                HirClassType::Custom(_) => return "(0)".to_string(),
            };
            format!("({func}({ptr}, {len}))")
        }
        HirCondition::And(a, b) => {
            let a = emit_condition(a, data_items);
            let b = emit_condition(b, data_items);
            format!("({a} && {b})")
        }
        HirCondition::Or(a, b) => {
            let a = emit_condition(a, data_items);
            let b = emit_condition(b, data_items);
            format!("({a} || {b})")
        }
        HirCondition::Not(inner) => {
            let c = emit_condition(inner, data_items);
            format!("(!({c}))")
        }
    }
}

/// Build a map from sanitized file name to sanitized FILE STATUS variable name.
pub(crate) fn build_file_status_map(file_status_vars: &[HirFileInfo]) -> FileStatusMap {
    file_status_vars
        .iter()
        .map(|info| {
            (
                sanitize_name(&info.file_name),
                sanitize_name(&info.status_var),
            )
        })
        .collect()
}

/// Emit a FILE STATUS variable update after a file I/O operation.
/// `fs_val` is the C expression (typically `_fs`) holding the uint32_t status.
pub(crate) fn emit_file_status_update(
    out: &mut String,
    file_c_name: &str,
    fs_val: &str,
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    declarative_mode_expr: &str,
    pad: &str,
) {
    if let Some(status_var) = fs_map.get(file_c_name) {
        // Convert numeric status code to 2-digit string: e.g. 0 → "00", 10 → "10"
        // Use an intermediate buffer + memcpy so this works even when the status
        // variable is a union (group item with REDEFINES) rather than a plain char[].
        out.push_str(&format!(
            "{pad}{{ char _fs_buf[4]; snprintf(_fs_buf, sizeof(_fs_buf), \"%02u\", (unsigned){fs_val}); memcpy(&{status_var}, _fs_buf, 2); }}\n"
        ));
    }
    if has_declaratives {
        let dispatch_fn = with_active_context(|ctx| ctx.file_declarative_dispatch_fn().to_string());
        out.push_str(&format!(
            "{pad}{dispatch_fn}(\"{file_c_name}\", {declarative_mode_expr}, {fs_val});\n"
        ));
        out.push_str(&format!(
            "{pad}if ({fs_val} != 0 && _goto_target) goto _goto_dispatch;\n"
        ));
    }
}

/// Collect all CALL target program names across the program body and
/// paragraphs. Returns sanitized, unique C identifiers for weak forward
/// declarations.
pub(crate) fn collect_call_targets(program: &HirProgram) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for stmt in &program.body {
        collect_call_targets_stmt(stmt, &mut targets);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_call_targets_stmt(stmt, &mut targets);
        }
    }
    for decl in &program.declaratives {
        for stmt in &decl.body {
            collect_call_targets_stmt(stmt, &mut targets);
        }
    }
    // Exclude nested program names (they are defined in this compilation unit)
    let nested_names: BTreeSet<String> = program
        .nested_programs
        .iter()
        .map(|p| sanitize_name(&p.name))
        .collect();
    targets
        .into_iter()
        .filter(|t| !nested_names.contains(t))
        .collect()
}

pub(crate) fn collect_call_targets_stmt(stmt: &HirStatement, targets: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::Call {
            program,
            on_exception,
            not_on_exception,
            ..
        } => {
            let prog_name = match program {
                HirExpr::Literal(HirLiteral::String(s)) => Some(sanitize_name(s)),
                _ => None,
            };
            if let Some(name) = prog_name {
                targets.insert(name);
            }
            for s in on_exception {
                collect_call_targets_stmt(s, targets);
            }
            for s in not_on_exception {
                collect_call_targets_stmt(s, targets);
            }
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_call_targets_stmt(s, targets);
            }
            for s in else_body {
                collect_call_targets_stmt(s, targets);
            }
        }
        HirStatement::Perform { kind, .. } => {
            if let HirPerformKind::Inline { body }
            | HirPerformKind::Times { body, .. }
            | HirPerformKind::Until { body, .. }
            | HirPerformKind::Varying { body, .. } = kind.as_ref()
            {
                for s in body {
                    collect_call_targets_stmt(s, targets);
                }
            }
        }
        _ => {}
    }
}

/// Collect all file names referenced in file I/O statements across
/// the program body,  and nested constructs. Returns a
/// sorted, deduplicated list of file names.
pub(crate) fn collect_file_names(program: &HirProgram) -> Vec<String> {
    let mut names = BTreeSet::new();
    for stmt in &program.body {
        collect_file_names_stmt(stmt, &mut names);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_file_names_stmt(stmt, &mut names);
        }
    }
    for nested in &program.nested_programs {
        names.extend(collect_file_names(nested));
    }
    names.into_iter().collect()
}

pub(crate) fn collect_file_names_stmt(stmt: &HirStatement, names: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::Open { entries, .. } => {
            for entry in entries {
                names.insert(entry.file_name.to_string());
            }
        }
        HirStatement::Close { files, .. } => {
            for f in files {
                names.insert(f.to_string());
            }
        }
        HirStatement::Read {
            file_name,
            at_end,
            not_at_end,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for s in not_at_end {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Write {
            file_name,
            record_name,
            ..
        } => {
            if file_name.is_empty() {
                names.insert(record_name.to_string());
            } else {
                names.insert(file_name.to_string());
            }
        }
        HirStatement::Rewrite {
            file_name,
            record_name,
            ..
        } => {
            if file_name.is_empty() {
                names.insert(record_name.to_string());
            } else {
                names.insert(file_name.to_string());
            }
        }
        HirStatement::Delete { file_name, .. } => {
            names.insert(file_name.to_string());
        }
        HirStatement::Sort {
            file_name,
            using,
            giving,
            ..
        } => {
            names.insert(file_name.to_string());
            for u in using {
                names.insert(u.to_string());
            }
            for g in giving {
                names.insert(g.to_string());
            }
        }
        HirStatement::Search {
            at_end,
            when_clauses,
            ..
        } => {
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for w in when_clauses {
                for s in &w.body {
                    collect_file_names_stmt(s, names);
                }
            }
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_file_names_stmt(s, names);
            }
            for s in else_body {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Perform { kind, .. } => {
            let body = match kind.as_ref() {
                HirPerformKind::Inline { body } => body.as_slice(),
                HirPerformKind::Times { body, .. } => body.as_slice(),
                HirPerformKind::Until { body, .. } => body.as_slice(),
                HirPerformKind::Varying { body, .. } => body.as_slice(),
                HirPerformKind::ProcedureName { .. } => &[],
            };
            for s in body {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Start {
            file_name,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in invalid_key {
                collect_file_names_stmt(s, names);
            }
            for s in not_invalid_key {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Return {
            file_name,
            at_end,
            not_at_end,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for s in not_at_end {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Merge {
            file_name,
            using,
            giving,
            ..
        } => {
            names.insert(file_name.to_string());
            for f in using {
                names.insert(f.to_string());
            }
            for f in giving {
                names.insert(f.to_string());
            }
        }
        HirStatement::Release { record_name, .. } => {
            names.insert(record_name.to_string());
        }
        _ => {}
    }
}

/// Collect unique XML PARSE processing procedure names from the program.
pub(crate) fn collect_xml_parse_procedures(program: &HirProgram) -> Vec<String> {
    let mut procs = BTreeSet::new();
    for stmt in &program.body {
        collect_xml_parse_stmt(stmt, &mut procs);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_xml_parse_stmt(stmt, &mut procs);
        }
    }
    procs.into_iter().collect()
}

pub(crate) fn collect_xml_parse_stmt(stmt: &HirStatement, procs: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::XmlParse {
            processing_procedure,
            ..
        } => {
            procs.insert(sanitize_name(processing_procedure));
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_xml_parse_stmt(s, procs);
            }
            for s in else_body {
                collect_xml_parse_stmt(s, procs);
            }
        }
        HirStatement::Perform { kind, .. } => {
            let body = match kind.as_ref() {
                HirPerformKind::Inline { body } => body.as_slice(),
                HirPerformKind::Times { body, .. } => body.as_slice(),
                HirPerformKind::Until { body, .. } => body.as_slice(),
                HirPerformKind::Varying { body, .. } => body.as_slice(),
                HirPerformKind::ProcedureName { .. } => &[],
            };
            for s in body {
                collect_xml_parse_stmt(s, procs);
            }
        }
        _ => {}
    }
}

/// Find the record length for a file/record name by looking up
/// the data item with a matching (sanitized) name. Returns the
/// size in bytes (default 80 if not found).
pub(crate) fn find_record_len(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    let base_size = find_data_item_size(c_name, data_items);
    // If this record is the primary record of an FD with multiple 01-level
    // records, return the max size so the runtime allocates enough buffer.
    let fd_max = with_active_context(|ctx| ctx.fd_max_record_size(c_name));
    if let Some(max_size) = fd_max {
        if max_size > base_size {
            return max_size;
        }
    }
    base_size
}

/// Find the OCCURS count for a table by its sanitized C name.
/// Returns a reasonable default (10) if the item is not found.
pub(crate) fn find_occurs_count(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            return item.occurs.unwrap_or(10);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_occurs_count_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    10
}

pub(crate) fn find_occurs_count_in(c_name: &str, members: &[HirDataItem]) -> u32 {
    for item in members {
        if sanitize_name(&item.name) == c_name {
            return item.occurs.unwrap_or(0);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_occurs_count_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    0
}

/// Find the first INDEXED BY name for a given table (OCCURS item) name.
pub(crate) fn find_first_index_name(c_name: &str, data_items: &[HirDataItem]) -> Option<String> {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            if let Some(first) = item.indexed_by.first() {
                return Some(sanitize_name(first));
            }
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_first_index_name(c_name, members) {
                return Some(found);
            }
        }
    }
    None
}

/// Find the byte size of a data item by its sanitized C name.
/// Returns a reasonable default (80) if the item is not found.
pub(crate) fn find_data_item_size(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    // Extract leaf name from complex expressions like
    // "TABLE.members._m_FOO[(I)-1].members._m_BAR"
    let lookup_name = if c_name.contains('[') || c_name.contains(".members.") {
        if let Some(pos) = c_name.rfind(".members._m_") {
            let leaf = &c_name[pos + ".members._m_".len()..];
            let leaf_name = if let Some(br) = leaf.find('[') {
                &leaf[..br]
            } else {
                leaf
            };
            if !leaf_name.is_empty() {
                leaf_name
            } else {
                c_name
            }
        } else if let Some(br) = c_name.find('[') {
            &c_name[..br]
        } else {
            c_name
        }
    } else {
        c_name
    };

    // Check cache first
    let cached = with_active_context(|ctx| ctx.data_item_size(lookup_name));
    if let Some(size) = cached {
        return size;
    }

    if let Some(item) = find_data_item_by_c_name(lookup_name, data_items)
        .or_else(|| find_data_item_by_c_name(c_name, data_items))
    {
        return data_item_byte_size(&item.data_type);
    }

    // Handle qualified C names like "WS_DST__FIELD_B"
    if lookup_name.contains("__") {
        if let Some(pos) = lookup_name.find("__") {
            let group_c = &lookup_name[..pos];
            let member_c = &lookup_name[pos + 2..];
            // Try cache for the member part
            let member_cached = with_active_context(|ctx| ctx.data_item_size(member_c));
            if let Some(size) = member_cached {
                return size;
            }
            for item in data_items {
                if sanitize_name(&item.name) == group_c {
                    if let HirType::Group { members, .. } = &item.data_type {
                        let found = find_data_item_size_in(member_c, members);
                        if found > 0 {
                            return found;
                        }
                    }
                }
            }
        }
    }

    // Fallback to recursive search (shouldn't normally happen after init)
    let found = find_data_item_size_in(lookup_name, data_items);
    if found > 0 {
        return found;
    }
    // If lookup_name differs from c_name, try the original c_name as well
    if lookup_name != c_name {
        let found2 = find_data_item_size_in(c_name, data_items);
        if found2 > 0 {
            return found2;
        }
    }
    80 // Default record length
}

pub(crate) fn find_data_item_size_in(c_name: &str, items: &[HirDataItem]) -> u32 {
    for item in items {
        let item_c_name = sanitize_name(&item.name);
        if item_c_name == c_name {
            if let Some((from, _thru)) = &item.renames {
                let from_c = sanitize_name(from);
                if from_c != c_name {
                    let renamed_size = find_data_item_size_in(&from_c, items);
                    if renamed_size > 0 {
                        return renamed_size;
                    }
                }
            }
            return data_item_byte_size(&item.data_type);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_data_item_size_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    0
}

pub(crate) fn find_data_item_storage_size(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    let lookup = extract_leaf_member(c_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) {
        return data_item_byte_size(&item.data_type);
    }
    find_data_item_size(c_name, data_items)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) item_len: u32,
    pub(crate) stride: u32,
    pub(crate) count: u32,
    pub(crate) area_len: u32,
}

impl Layout {
    fn scalar(len: u32) -> Self {
        Self {
            item_len: len,
            stride: len,
            count: 1,
            area_len: len,
        }
    }
}

pub(crate) fn find_data_item_layout(c_name: &str, data_items: &[HirDataItem]) -> Layout {
    if let Some(item) = find_data_item_by_c_name(c_name, data_items) {
        let item_len = data_item_storage_size(item);
        let count = item.occurs.unwrap_or(1);
        return Layout {
            item_len,
            stride: item_len,
            count,
            area_len: item_len.saturating_mul(count),
        };
    }
    let lookup = extract_leaf_member(c_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) {
        let item_len = data_item_storage_size(item);
        let count = item.occurs.unwrap_or(1);
        return Layout {
            item_len,
            stride: item_len,
            count,
            area_len: item_len.saturating_mul(count),
        };
    }
    Layout::scalar(find_data_item_size(c_name, data_items))
}

pub(crate) fn find_data_item_element_size(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    find_data_item_layout(c_name, data_items).item_len
}

pub(crate) fn find_data_item_area_size(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    find_data_item_layout(c_name, data_items).area_len
}

pub(crate) fn find_data_item_occurs_count(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    let lookup = extract_leaf_member(c_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) {
        return item.occurs.unwrap_or(1);
    }
    1
}

pub(crate) fn find_data_item_stride(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    let lookup = extract_leaf_member(c_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) {
        return data_item_storage_size(item);
    }
    find_data_item_element_size(c_name, data_items)
}

/// Compute the byte size of an HIR type.
pub(crate) fn data_item_byte_size(data_type: &HirType) -> u32 {
    match data_type {
        HirType::Alphanumeric { size } => *size,
        HirType::Numeric { size, .. } => *size,
        HirType::Group { size, .. } => *size,
        HirType::Comp3 { size, .. } => *size,
        HirType::Binary { size } => *size,
        HirType::Index => 8,
        HirType::Pointer => 8,
        HirType::Boolean => 1,
        HirType::FloatShort => 4,
        HirType::FloatLong => 8,
        HirType::FloatExtended => 16,
        HirType::National { size } => size * 2, // UTF-16: 2 bytes per character
    }
}

pub(crate) fn data_item_storage_size(item: &HirDataItem) -> u32 {
    match item.data_type {
        HirType::Numeric { size, .. } if item.sign.is_some_and(|sign| sign.separate) => size + 1,
        _ => data_item_byte_size(&item.data_type),
    }
}

/// Find the byte offset and size of a field within a record structure.
/// Returns (offset, size) if found, None otherwise.
pub(crate) fn find_field_offset_and_size(
    field_name: &str,
    record_name: &str,
    data_items: &[HirDataItem],
) -> Option<(u32, u32)> {
    let field_c = sanitize_name(field_name);
    let record_c = sanitize_name(record_name);
    for item in data_items {
        if sanitize_name(&item.name) == record_c {
            if let HirType::Group { members, .. } = &item.data_type {
                return find_field_in_group(&field_c, members, 0);
            }
        }
    }
    None
}

fn find_field_in_group(
    field_c: &str,
    members: &[HirDataItem],
    base_offset: u32,
) -> Option<(u32, u32)> {
    let mut offset = base_offset;
    for item in members {
        let item_c = sanitize_name(&item.name);
        let item_size = data_item_byte_size(&item.data_type);
        let item_offset = if item.redefines.is_some() {
            offset.saturating_sub(item_size)
        } else {
            offset
        };
        if item_c == field_c {
            return Some((item_offset, item_size));
        }
        if let HirType::Group { members: sub, .. } = &item.data_type {
            if let Some(found) = find_field_in_group(field_c, sub, item_offset) {
                return Some(found);
            }
        }
        // Only advance offset for non-REDEFINES items
        if item.redefines.is_none() {
            offset += item_size;
        }
    }
    None
}

/// Convert a COBOL data name to a valid C identifier.
///
/// COBOL names use hyphens which are not valid in C, so we replace
/// them with underscores. Additionally, names starting with a digit
/// are prefixed with `cob_`, and C reserved words are prefixed to
/// avoid collisions.
pub(crate) fn sanitize_name(name: impl AsRef<str>) -> String {
    let mut result = name.as_ref().replace("::", "__").replace('-', "_");
    // C identifiers cannot start with a digit
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert_str(0, "cob_");
    }
    // Avoid C reserved words
    match result.as_str() {
        "auto" | "break" | "case" | "char" | "const" | "continue" | "default" | "do" | "double"
        | "else" | "enum" | "extern" | "float" | "for" | "goto" | "if" | "int" | "long"
        | "register" | "return" | "short" | "signed" | "sizeof" | "static" | "struct"
        | "switch" | "typedef" | "union" | "unsigned" | "void" | "volatile" | "while"
        | "inline" | "restrict" | "_Bool" | "_Complex" | "_Imaginary" | "main" => {
            result.insert_str(0, "cob_");
        }
        _ => {}
    }
    result
}

/// Resolve the FD/SD record buffer variable name for a file.
/// Returns the first record name from the FILE_RECORD_MAP if available,
/// otherwise falls back to the file name itself.
pub(crate) fn resolve_file_record(sanitized_file_name: &str) -> String {
    with_active_context(|ctx| ctx.resolve_file_record(sanitized_file_name))
}

/// Determine the sort key type for a field (0=alpha, 1=signed binary, 2=unsigned binary, 3=display numeric).
pub(crate) fn sort_key_type_for_field(field_name: &str, data_items: &[HirDataItem]) -> u8 {
    let field_c = sanitize_name(field_name);
    if let Some(item) = find_original_data_item_by_sanitized_name(&field_c, data_items) {
        match &item.data_type {
            HirType::Binary { size } => {
                // Check if signed from the field name context or size
                // COBOL COMP fields with S picture are signed
                if *size <= 8 {
                    1 // signed binary (default for COMP)
                } else {
                    0
                }
            }
            HirType::Numeric { .. } => 3, // display numeric (char[]) - always display format in sort buffer
            HirType::Comp3 { .. } => 1,   // int64_t - binary in sort buffer
            _ => 0,                       // alphanumeric
        }
    } else {
        0
    }
}

/// Escape special characters for use in a C string literal.
pub(crate) fn escape_c_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            c => escaped.push(c),
        }
    }
    escaped
}
