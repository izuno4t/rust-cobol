use super::*;
use cobol_hir::{
    HirDataName, HirParagraphId, HirParagraphKind, HirPerformTest, HirReceiveMode, HirSignPosition,
    HirType, HirWriteAdvancing,
};

pub(crate) struct StmtEmitEnv<'a> {
    pub(crate) data_items: &'a [HirDataItem],
    pub(crate) paragraphs: &'a [HirParagraph],
    pub(crate) fs_map: &'a FileStatusMap,
    pub(crate) has_declaratives: bool,
    pub(crate) ctx: &'a CodegenContext,
    pub(crate) current_paragraph: Option<HirParagraphId>,
}

pub(crate) fn emit_statement(
    out: &mut String,
    stmt: &HirStatement,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    with_active_context(|ctx| {
        let env = StmtEmitEnv {
            data_items,
            paragraphs,
            fs_map,
            has_declaratives,
            ctx,
            current_paragraph: None,
        };
        emit_statement_with_ctx(out, stmt, &env, indent)
    })
}

pub(crate) fn emit_statement_with_ctx(
    out: &mut String,
    stmt: &HirStatement,
    env: &StmtEmitEnv<'_>,
    indent: usize,
) {
    let data_items = env.data_items;
    let paragraphs = env.paragraphs;
    let fs_map = env.fs_map;
    let has_declaratives = env.has_declaratives;
    let ctx = env.ctx;
    let current_paragraph = env.current_paragraph;
    let emit_expr = |expr| super::emit_expr_with_ctx(expr, ctx);
    let emit_condition = |cond, items| super::emit_condition_with_ctx(cond, items, ctx);
    let pad = "    ".repeat(indent);
    match stmt {
        HirStatement::Display {
            operands,
            no_advancing,
            ..
        } => {
            for (i, op) in operands.iter().enumerate() {
                if i > 0 {
                    // COBOL DISPLAY separates operands with a space by default
                    // (some implementations don't; we follow the common convention)
                }
                emit_display_operand(out, op, data_items, &pad);
            }
            if !no_advancing {
                out.push_str(&format!("{pad}cobol_display_newline();\n"));
            } else {
                out.push_str(&format!("{pad}cobol_display_flush();\n"));
            }
        }
        HirStatement::StopLiteral { operand, .. } => {
            emit_display_operand(out, operand, data_items, &pad);
            out.push_str(&format!("{pad}cobol_display_newline();\n"));
            out.push_str(&format!("{pad}cobol_stop_literal();\n"));
        }
        HirStatement::Move { from, to, .. } => {
            emit_debug_numeric_identifier_source_event(out, &pad, from, data_items);
            for target in to {
                match target {
                    HirMoveTarget::DataRef(data_ref) => {
                        if let Some(refmod) = &data_ref.refmod {
                            emit_move_to_refmod(
                                out,
                                from,
                                &data_ref.name,
                                &refmod.start,
                                &refmod.length.as_deref().cloned(),
                                data_items,
                                &pad,
                            );
                        } else {
                            let c_target = data_ref_base_c_name(data_ref);
                            emit_move_to(out, from, &data_ref.name, &c_target, data_items, &pad);
                            if !data_ref.subscripts.is_empty() {
                                emit_debug_subscript_values(
                                    out,
                                    &pad,
                                    &data_ref.subscripts,
                                    data_items,
                                );
                            }
                            emit_debug_numeric_identifier_target_event(
                                out,
                                &pad,
                                &data_ref.name,
                                &c_target,
                                data_items,
                            );
                        }
                    }
                    HirMoveTarget::Variable(name) => {
                        let c_target = data_name_to_c_name(name);
                        emit_move_to(out, from, name, &c_target, data_items, &pad);
                        emit_debug_numeric_identifier_target_event(
                            out, &pad, name, &c_target, data_items,
                        );
                    }
                    HirMoveTarget::ReferenceModification {
                        variable,
                        start,
                        length,
                    } => {
                        emit_move_to_refmod(out, from, variable, start, length, data_items, &pad);
                    }
                    HirMoveTarget::Subscript {
                        variable,
                        subscripts,
                    } => {
                        let c_target = emit_subscript_access(variable, subscripts);
                        emit_move_to(out, from, variable, &c_target, data_items, &pad);
                        emit_debug_subscript_values(out, &pad, subscripts, data_items);
                        emit_debug_numeric_identifier_target_event(
                            out, &pad, variable, &c_target, data_items,
                        );
                    }
                }
            }
        }
        HirStatement::SetConditionTrue { assignments, .. } => {
            for (target, value) in assignments {
                match target {
                    HirMoveTarget::DataRef(data_ref) => {
                        let c_target = data_ref_base_c_name(data_ref);
                        emit_move_to(out, value, &data_ref.name, &c_target, data_items, &pad);
                    }
                    HirMoveTarget::Variable(name) => {
                        let c_target = data_name_to_c_name(name);
                        emit_move_to(out, value, name, &c_target, data_items, &pad);
                    }
                    HirMoveTarget::ReferenceModification {
                        variable,
                        start,
                        length,
                    } => {
                        emit_move_to_refmod(out, value, variable, start, length, data_items, &pad);
                    }
                    HirMoveTarget::Subscript {
                        variable,
                        subscripts,
                    } => {
                        let c_target = emit_subscript_access(variable, subscripts);
                        emit_move_to(out, value, variable, &c_target, data_items, &pad);
                    }
                }
            }
        }
        HirStatement::MoveCorresponding {
            from,
            from_subscripts,
            to,
            to_subscripts,
            ..
        } => {
            emit_corresponding_move(
                out,
                from,
                from_subscripts,
                to,
                to_subscripts,
                data_items,
                &pad,
            );
        }
        HirStatement::AddCorresponding {
            from,
            to,
            rounded,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(
                out,
                from,
                to,
                "+",
                *rounded,
                has_size_error,
                data_items,
                &pad,
            );
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::SubtractCorresponding {
            from,
            to,
            rounded,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(
                out,
                from,
                to,
                "-",
                *rounded,
                has_size_error,
                data_items,
                &pad,
            );
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Compute {
            targets,
            target_rounded,
            expr,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            for (idx, target) in targets.iter().enumerate() {
                let rounded = target_rounded.get(idx).copied().unwrap_or(false);
                let c_target = emit_expr(target);
                let target_name = expr_data_name(target);
                let target_is_decimal = target_name
                    .and_then(|name| find_data_item_by_name(name, data_items))
                    .is_some_and(|i| needs_decimal(&i.data_type));
                if has_size_error {
                    if let Some((target_size, target_scale, _)) =
                        display_numeric_c_expr_metadata(&c_target, data_items)
                    {
                        let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                        let c_target_ptr = display_numeric_ptr(&c_target);
                        out.push_str(&format!(
                            "{pad}{{ int64_t _prev = cobol_display_to_int64({c_target_const_ptr}, {target_size}); "
                        ));
                        out.push_str(&decimal_init_statement(
                            "_compute_result",
                            Some(expr),
                            data_items,
                        ));
                        out.push_str(&decimal_rescale_to_scale_statement(
                            "_compute_result",
                            target_scale,
                            rounded,
                        ));
                        if let Some(max_val) =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items))
                        {
                            out.push_str(&format!(
                                "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                 else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }} "
                            ));
                        } else {
                            out.push_str(&format!(
                                "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); "
                            ));
                        }
                        out.push_str("(void)_prev; }\n");
                    } else {
                        let c_expr = emit_int_compatible_expr(expr, data_items);
                        emit_save_and_check_overflow(
                            out,
                            target_name.map_or("", HirDataName::as_str),
                            &c_target,
                            &c_expr,
                            data_items,
                            &pad,
                        );
                    }
                } else if let Some((target_size, target_scale, _)) =
                    display_numeric_c_expr_metadata(&c_target, data_items)
                {
                    let c_target_ptr = display_numeric_ptr(&c_target);
                    out.push_str(&format!("{pad}{{ "));
                    if target_scale > 0
                        && matches!(
                            expr,
                            HirExpr::BinaryOp {
                                op: HirBinOp::Div,
                                ..
                            }
                        )
                    {
                        let c_expr = emit_expr_as_double(expr);
                        out.push_str(&format!(
                            "CobolDecimal _compute_result = {{ .value = 0, .scale = 9, .size = 18, .is_signed = 1 }}; \
                             cobol_decimal_from_double({c_expr}, &_compute_result); "
                        ));
                    } else {
                        out.push_str(&decimal_init_statement(
                            "_compute_result",
                            Some(expr),
                            data_items,
                        ));
                    }
                    out.push_str(&decimal_rescale_to_scale_statement(
                        "_compute_result",
                        target_scale,
                        rounded,
                    ));
                    out.push_str(&format!(
                        "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }}\n"
                    ));
                } else if target_is_decimal {
                    if rounded
                        || matches!(
                            expr,
                            HirExpr::BinaryOp {
                                op: HirBinOp::Div,
                                ..
                            }
                        )
                    {
                        let c_expr = emit_expr_as_double(expr);
                        let metadata_init =
                            decimal_target_metadata_init_statement(target, &c_target, data_items);
                        out.push_str(&format!(
                            "{pad}{{ {metadata_init} CobolDecimal _compute_result = {{ .value = 0, .scale = 9, .size = 18, .is_signed = 1 }}; \
                             cobol_decimal_from_double({c_expr}, &_compute_result); "
                        ));
                        out.push_str(&decimal_rescale_to_target_statement(
                            "_compute_result",
                            &c_target,
                            rounded,
                        ));
                        out.push_str(&format!(
                            "{c_target}.value = _result; {c_target}.is_signed = {c_target}.is_signed || _compute_result.is_signed; }}\n"
                        ));
                    } else {
                        emit_assign_to_decimal(out, expr, &c_target, data_items, &pad);
                    }
                } else if rounded {
                    let c_expr = emit_expr_as_double(expr);
                    let rounded_expr = format!("((int64_t)llround({c_expr}))");
                    emit_store_int(out, &c_target, &rounded_expr, data_items, &pad);
                } else {
                    let c_expr = emit_int_compatible_expr(expr, data_items);
                    emit_store_int(out, &c_target, &c_expr, data_items, &pad);
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Add {
            operands,
            to,
            to_rounded,
            giving,
            giving_rounded,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let receiving_names: Vec<String> = if giving.is_empty() {
                to.iter()
                    .filter_map(expr_data_name)
                    .map(|name| name.name.to_string())
                    .collect()
            } else {
                Vec::new()
            };
            emit_debug_numeric_unique_source_events(
                out,
                &pad,
                operands,
                data_items,
                &receiving_names,
            );
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // ADD a b GIVING c d -> c = a + b, d = a + b
                // All operands + TO values are summed, result goes to GIVING targets
                let mut all_addends: Vec<String> = operands
                    .iter()
                    .map(|o| emit_int_compatible_expr(o, data_items))
                    .collect();
                for t in to {
                    all_addends.push(emit_int_compatible_expr(t, data_items));
                }
                let sum_expr = all_addends.join(" + ");
                for (idx, target) in giving.iter().enumerate() {
                    let _rounded = giving_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_expr_is_decimal(target, &c_target, data_items);
                    let target_pad = if has_size_error {
                        out.push_str(&format!("{pad}if (!_size_error) {{\n"));
                        format!("{pad}    ")
                    } else {
                        pad.clone()
                    };
                    let uses_decimal = operands.iter().chain(to.iter()).any(|o| {
                        is_decimal_expr(o, data_items)
                            || decimal_literal_parts(o).is_some_and(|(_, scale)| scale > 0)
                            || expr_is_scaled_display_numeric(o, data_items)
                    });
                    let numeric_edited_item = find_data_item_by_c_name(&c_target, data_items)
                        .or_else(|| find_data_item(&c_target, data_items))
                        .filter(|item| item.is_numeric_edited);
                    if target_is_decimal {
                        // For decimal GIVING, build a temp sum then assign
                        out.push_str(&format!("{target_pad}/* ADD GIVING decimal */\n"));
                        // Use first two addends as decimal add, then chain
                        emit_decimal_giving_add(
                            out,
                            operands,
                            to,
                            &c_target,
                            data_items,
                            &target_pad,
                            has_size_error,
                        );
                    } else if uses_decimal && numeric_edited_item.is_some() {
                        let item = numeric_edited_item.expect("checked is_some");
                        let terms: Vec<&HirExpr> = operands.iter().chain(to.iter()).collect();
                        if let Some((first_term, rest_terms)) = terms.split_first() {
                            out.push_str(&format!("{target_pad}{{ "));
                            out.push_str(&decimal_init_statement(
                                "_ag",
                                Some(first_term),
                                data_items,
                            ));
                            for (op_idx, op) in rest_terms.iter().enumerate() {
                                let tmp = format!("_ago{op_idx}");
                                out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                                out.push_str(&decimal_add_exact_statement("_ag", &tmp));
                            }
                            let max_int =
                                item.picture.as_deref().and_then(numeric_edited_integer_max);
                            if let (true, Some(max_int)) = (has_size_error, max_int) {
                                out.push_str(&format!(
                                    "int64_t _abs = llabs(_ag.value); \
                                     for (int32_t _i = 0; _i < _ag.scale; _i++) _abs /= 10; \
                                     if (_abs > {max_int}) {{ _size_error = 1; }} else {{ "
                                ));
                                emit_store_decimal_to_numeric_edited(
                                    out, &c_target, "_ag", item, _rounded, "",
                                );
                                out.push_str("} ");
                            } else {
                                emit_store_decimal_to_numeric_edited(
                                    out, &c_target, "_ag", item, _rounded, "",
                                );
                            }
                            out.push_str("}\n");
                        }
                    } else if uses_decimal {
                        let terms: Vec<&HirExpr> = operands.iter().chain(to.iter()).collect();
                        if let Some((first_term, rest_terms)) = terms.split_first() {
                            out.push_str(&format!("{target_pad}{{ "));
                            out.push_str(&decimal_init_statement(
                                "_ag",
                                Some(first_term),
                                data_items,
                            ));
                            for (op_idx, op) in rest_terms.iter().enumerate() {
                                let tmp = format!("_ago{op_idx}");
                                out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                                out.push_str(&decimal_add_exact_statement("_ag", &tmp));
                            }
                            let display_target =
                                display_numeric_c_expr_metadata(&c_target, data_items);
                            let target_scale = display_target
                                .map(|(_, scale, _)| scale)
                                .or_else(|| decimal_expr_scale(target, data_items))
                                .unwrap_or(0);
                            out.push_str(&decimal_rescale_to_scale_statement(
                                "_ag",
                                target_scale,
                                _rounded,
                            ));
                            if has_size_error {
                                if let Some(max_val) = get_pic_max(
                                    target_name.map_or("", HirDataName::as_str),
                                    data_items,
                                ) {
                                    out.push_str(&format!(
                                        "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                    ));
                                    if let Some((target_size, _, _)) = display_target {
                                        emit_store_display_numeric(
                                            out,
                                            "",
                                            "_result",
                                            &c_target,
                                            target_size,
                                            data_items,
                                        );
                                    } else {
                                        emit_store_int(out, &c_target, "_result", data_items, "");
                                    }
                                    out.push_str("} ");
                                } else if let Some((target_size, _, _)) = display_target {
                                    emit_store_display_numeric(
                                        out,
                                        "",
                                        "_result",
                                        &c_target,
                                        target_size,
                                        data_items,
                                    );
                                } else {
                                    emit_store_int(out, &c_target, "_result", data_items, "");
                                }
                            } else if let Some((target_size, _, _)) = display_target {
                                emit_store_display_numeric(
                                    out,
                                    "",
                                    "_result",
                                    &c_target,
                                    target_size,
                                    data_items,
                                );
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, "");
                            }
                            out.push_str("}\n");
                        }
                    } else if has_size_error {
                        emit_size_checked_int_assignment(
                            out,
                            &c_target,
                            &sum_expr,
                            target_name.map_or("", HirDataName::as_str),
                            data_items,
                            &target_pad,
                        );
                    } else {
                        emit_store_int(out, &c_target, &sum_expr, data_items, &target_pad);
                    }
                    if has_size_error {
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            } else {
                for (idx, target) in to.iter().enumerate() {
                    let rounded = to_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_expr_is_decimal(target, &c_target, data_items);
                    if target_is_decimal {
                        if rounded || has_size_error {
                            let display_target =
                                display_numeric_c_expr_metadata(&c_target, data_items);
                            let target_scale = display_target
                                .map(|(_, scale, _)| scale)
                                .or_else(|| decimal_expr_scale(target, data_items));
                            if let Some(target_scale) = target_scale {
                                out.push_str(&format!("{pad}{{ "));
                                out.push_str(&decimal_init_statement(
                                    "_ar",
                                    Some(target),
                                    data_items,
                                ));
                                for (op_idx, op) in operands.iter().enumerate() {
                                    let tmp = format!("_ao{op_idx}");
                                    out.push_str(&decimal_init_statement(
                                        &tmp,
                                        Some(op),
                                        data_items,
                                    ));
                                    out.push_str(&decimal_add_exact_statement("_ar", &tmp));
                                }
                                out.push_str(&decimal_rescale_to_scale_statement(
                                    "_ar",
                                    target_scale,
                                    rounded,
                                ));
                                if has_size_error {
                                    if let Some(max_val) = get_pic_max(
                                        target_name.map_or("", HirDataName::as_str),
                                        data_items,
                                    ) {
                                        out.push_str(&format!(
                                            "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                        ));
                                        if let Some((target_size, _, _)) = display_target {
                                            emit_store_display_numeric(
                                                out,
                                                "",
                                                "_result",
                                                &c_target,
                                                target_size,
                                                data_items,
                                            );
                                        } else {
                                            out.push_str(&format!(
                            "{c_target}.value = _result; {c_target}.scale = {target_scale}; "
                        ));
                                        }
                                        out.push_str("} }\n");
                                    } else if let Some((target_size, _, _)) = display_target {
                                        emit_store_display_numeric(
                                            out,
                                            "",
                                            "_result",
                                            &c_target,
                                            target_size,
                                            data_items,
                                        );
                                        out.push_str("}\n");
                                    } else {
                                        out.push_str(&format!(
                            "{c_target}.value = _result; {c_target}.scale = {target_scale}; }}\n"
                        ));
                                    }
                                } else if let Some((target_size, _, _)) = display_target {
                                    emit_store_display_numeric(
                                        out,
                                        "",
                                        "_result",
                                        &c_target,
                                        target_size,
                                        data_items,
                                    );
                                    out.push_str("}\n");
                                } else {
                                    out.push_str(&format!(
                        "{c_target}.value = _result; {c_target}.scale = {target_scale}; }}\n"
                    ));
                                }
                            } else {
                                for op in operands {
                                    emit_decimal_arith(
                                        out,
                                        &c_target,
                                        op,
                                        "cobol_decimal_add",
                                        data_items,
                                        &pad,
                                    );
                                }
                            }
                        } else {
                            for op in operands {
                                if !emit_fast_decimal_add_assign(
                                    out, &c_target, target, op, data_items, &pad,
                                ) {
                                    emit_decimal_arith(
                                        out,
                                        &c_target,
                                        op,
                                        "cobol_decimal_add",
                                        data_items,
                                        &pad,
                                    );
                                }
                            }
                        }
                    } else {
                        let uses_decimal = operands.iter().any(|o| {
                            is_decimal_expr(o, data_items)
                                || decimal_literal_parts(o).is_some_and(|(_, scale)| scale > 0)
                                || expr_is_scaled_display_numeric(o, data_items)
                        });
                        if uses_decimal && rounded {
                            out.push_str(&format!("{pad}{{ "));
                            out.push_str(&decimal_init_statement("_ar", Some(target), data_items));
                            for (op_idx, op) in operands.iter().enumerate() {
                                let tmp = format!("_ao{op_idx}");
                                out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                                out.push_str(&decimal_add_exact_statement("_ar", &tmp));
                            }
                            out.push_str(
                                "int64_t _result = _ar.value; \
                                 if (_ar.scale > 0) { \
                                     int64_t _factor = 1; \
                                     for (int32_t _i = 0; _i < _ar.scale; _i++) _factor *= 10; \
                                     _result = (_ar.value >= 0) ? ((_ar.value + (_factor / 2)) / _factor) : ((_ar.value - (_factor / 2)) / _factor); \
                                 } else if (_ar.scale < 0) { \
                                     for (int32_t _i = 0; _i < -_ar.scale; _i++) _result *= 10; \
                                 } ",
                            );
                            if has_size_error {
                                if let Some(max_val) = get_pic_max(
                                    target_name.map_or("", HirDataName::as_str),
                                    data_items,
                                ) {
                                    out.push_str(&format!(
                                        "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                    ));
                                    emit_store_int(out, &c_target, "_result", data_items, "");
                                    out.push_str("} ");
                                } else {
                                    emit_store_int(out, &c_target, "_result", data_items, "");
                                }
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, "");
                            }
                            out.push_str("}\n");
                            continue;
                        }
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            let current = if let Some((disp_size, _, _)) =
                                display_numeric_c_expr_metadata(&c_target, data_items)
                            {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                format!("cobol_display_to_int64({c_target_const_ptr}, {disp_size})")
                            } else if let Some(disp_size) = grp_display_size(&c_target, data_items)
                            {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                format!("cobol_display_to_int64({c_target_const_ptr}, {disp_size})")
                            } else if find_data_item_by_c_name(&c_target, data_items)
                                .or_else(|| find_data_item(&c_target, data_items))
                                .is_some_and(|item| item.is_numeric_edited)
                            {
                                let size = find_data_item_size(&c_target, data_items);
                                format!("cobol_func_numval((const uint8_t*){c_target}, {size})")
                            } else {
                                target_name
                                    .and_then(|name| find_data_item_by_name(name, data_items))
                                    .or_else(|| find_data_item_by_c_name(&c_target, data_items))
                                    .or_else(|| find_data_item(&c_target, data_items))
                                    .filter(|item| item.scale_adjustment != 0)
                                    .map(|item| {
                                        apply_scale_adjustment_to_read(
                                            &c_target,
                                            item.scale_adjustment,
                                        )
                                    })
                                    .unwrap_or_else(|| c_target.clone())
                            };
                            let result_expr = format!("{current} + ({sum_expr})");
                            emit_size_checked_int_assignment(
                                out,
                                &c_target,
                                &result_expr,
                                target_name.map_or("", HirDataName::as_str),
                                data_items,
                                &pad,
                            );
                        } else {
                            emit_store_int_op(out, &c_target, "+", &sum_expr, data_items, &pad);
                        }
                        if let Some(target_name) = target_name {
                            if operands.iter().any(|operand| {
                                expr_data_name(operand)
                                    .is_some_and(|name| name.name == target_name.name)
                            }) {
                                emit_debug_numeric_identifier_target_event(
                                    out,
                                    &pad,
                                    target_name,
                                    &c_target,
                                    data_items,
                                );
                            }
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Subtract {
            operands,
            from,
            from_rounded,
            giving,
            giving_rounded,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // SUBTRACT a FROM b GIVING c -> c = b - a
                let sub_vals: Vec<String> = operands
                    .iter()
                    .map(|o| emit_int_compatible_expr(o, data_items))
                    .collect();
                let sub_expr = sub_vals.join(" + ");
                let uses_decimal = from.first().is_some_and(|f| {
                    is_decimal_expr(f, data_items)
                        || decimal_literal_parts(f).is_some()
                        || expr_is_scaled_display_numeric(f, data_items)
                }) || operands.iter().any(|o| {
                    is_decimal_expr(o, data_items)
                        || decimal_literal_parts(o).is_some()
                        || expr_is_scaled_display_numeric(o, data_items)
                });
                // The FROM value is the minuend
                let from_val = if let Some(f) = from.first() {
                    emit_int_compatible_expr(f, data_items)
                } else {
                    "0".to_string()
                };
                for (idx, target) in giving.iter().enumerate() {
                    if has_size_error {
                        out.push_str(&format!("{pad}if (!_size_error) {{\n"));
                    }
                    let rounded = giving_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_expr_is_decimal(target, &c_target, data_items);
                    let numeric_edited_item = find_data_item_by_c_name(&c_target, data_items)
                        .or_else(|| find_data_item(&c_target, data_items))
                        .filter(|item| item.is_numeric_edited);
                    if target_is_decimal {
                        // SUBTRACT GIVING decimal: result = from - sub
                        if uses_decimal {
                            out.push_str(&format!("{pad}{{ "));
                            out.push_str(&decimal_init_statement("_sr", from.first(), data_items));
                            for (op_idx, op) in operands.iter().enumerate() {
                                let tmp = format!("_so{op_idx}");
                                out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                                out.push_str(&decimal_subtract_exact_statement("_sr", &tmp));
                            }
                            if let Some((target_size, target_scale, _)) =
                                display_numeric_c_expr_metadata(&c_target, data_items)
                            {
                                out.push_str(&decimal_rescale_to_scale_statement(
                                    "_sr",
                                    target_scale,
                                    rounded,
                                ));
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                if has_size_error {
                                    let target_name_str =
                                        target_name.map_or("", HirDataName::as_str);
                                    if let Some(max_val) = get_pic_max(target_name_str, data_items)
                                    {
                                        out.push_str(&format!(
                                            "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                             else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }} "
                                        ));
                                    } else {
                                        out.push_str(&format!(
                                            "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); "
                                        ));
                                    }
                                } else {
                                    out.push_str(&format!(
                                        "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); "
                                    ));
                                }
                            } else {
                                out.push_str(&decimal_target_metadata_init_statement(
                                    target, &c_target, data_items,
                                ));
                                out.push_str(&decimal_rescale_to_target_statement(
                                    "_sr", &c_target, rounded,
                                ));
                                if has_size_error {
                                    let target_name_str =
                                        target_name.map_or("", HirDataName::as_str);
                                    if let Some(max_val) = get_pic_max(target_name_str, data_items)
                                    {
                                        out.push_str(&format!(
                                            "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                             else {{ {c_target}.value = _result; }} "
                                        ));
                                    } else {
                                        out.push_str(&format!("{c_target}.value = _result; "));
                                    }
                                } else {
                                    out.push_str(&format!("{c_target}.value = _result; "));
                                }
                            }
                            out.push_str("}\n");
                        } else {
                            out.push_str(&format!(
                                "{pad}cobol_decimal_from_int(\
                                 {from_val} - ({sub_expr}), 0, &{c_target});\n"
                            ));
                        }
                    } else if uses_decimal && numeric_edited_item.is_some() {
                        let item = numeric_edited_item.expect("checked is_some");
                        out.push_str(&format!("{pad}{{ "));
                        out.push_str(&decimal_init_statement("_sr", from.first(), data_items));
                        for (op_idx, op) in operands.iter().enumerate() {
                            let tmp = format!("_so{op_idx}");
                            out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                            out.push_str(&decimal_subtract_exact_statement("_sr", &tmp));
                        }
                        let max_int = item.picture.as_deref().and_then(numeric_edited_integer_max);
                        if let (true, Some(max_int)) = (has_size_error, max_int) {
                            out.push_str(&format!(
                                "int64_t _abs = llabs(_sr.value); \
                                 for (int32_t _i = 0; _i < _sr.scale; _i++) _abs /= 10; \
                                 if (_abs > {max_int}) {{ _size_error = 1; }} else {{ "
                            ));
                            emit_store_decimal_to_numeric_edited(
                                out, &c_target, "_sr", item, rounded, "",
                            );
                            out.push_str("} ");
                        } else {
                            emit_store_decimal_to_numeric_edited(
                                out, &c_target, "_sr", item, rounded, "",
                            );
                        }
                        out.push_str("}\n");
                    } else if uses_decimal {
                        out.push_str(&format!("{pad}{{ "));
                        out.push_str(&decimal_init_statement("_sr", from.first(), data_items));
                        for (op_idx, op) in operands.iter().enumerate() {
                            let tmp = format!("_so{op_idx}");
                            out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                            out.push_str(&decimal_subtract_exact_statement("_sr", &tmp));
                        }
                        if rounded {
                            out.push_str(
                                "int64_t _result = _sr.value; \
                                 if (_sr.scale > 0) { \
                                     int64_t _factor = 1; \
                                     for (int32_t _i = 0; _i < _sr.scale; _i++) _factor *= 10; \
                                     _result = (_sr.value >= 0) ? ((_sr.value + (_factor / 2)) / _factor) : ((_sr.value - (_factor / 2)) / _factor); \
                                 } else if (_sr.scale < 0) { \
                                     for (int32_t _i = 0; _i < -_sr.scale; _i++) _result *= 10; \
                                 } ",
                            );
                        } else {
                            out.push_str(
                                "int64_t _result = _sr.value; \
                                 if (_sr.scale > 0) { \
                                     int64_t _factor = 1; \
                                     for (int32_t _i = 0; _i < _sr.scale; _i++) _factor *= 10; \
                                     _result = _sr.value / _factor; \
                                 } else if (_sr.scale < 0) { \
                                     for (int32_t _i = 0; _i < -_sr.scale; _i++) _result *= 10; \
                                 } ",
                            );
                        }
                        let target_name_str = target_name.map_or("", HirDataName::as_str);
                        if has_size_error {
                            if let Some(max_val) = get_pic_max(target_name_str, data_items) {
                                out.push_str(&format!(
                                    "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                ));
                                emit_store_int(out, &c_target, "_result", data_items, "");
                                out.push_str("} ");
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, "");
                            }
                        } else {
                            emit_store_int(out, &c_target, "_result", data_items, "");
                        }
                        out.push_str("}\n");
                    } else if has_size_error {
                        let result_expr = format!("{from_val} - ({sub_expr})");
                        emit_size_checked_int_assignment(
                            out,
                            &c_target,
                            &result_expr,
                            target_name.map_or("", HirDataName::as_str),
                            data_items,
                            &pad,
                        );
                    } else {
                        let result_expr = format!("{from_val} - ({sub_expr})");
                        emit_store_int(out, &c_target, &result_expr, data_items, &pad);
                    }
                    if has_size_error {
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            } else {
                for (idx, target) in from.iter().enumerate() {
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_expr_is_decimal(target, &c_target, data_items);
                    let target_item = target_name
                        .and_then(|name| find_data_item_by_name(name, data_items))
                        .or_else(|| find_data_item_by_c_name(&c_target, data_items))
                        .or_else(|| find_data_item(&c_target, data_items));
                    let scale_adjustment = target_item.map_or(0, |item| item.scale_adjustment);
                    let rounded = from_rounded.get(idx).copied().unwrap_or(false);
                    let uses_decimal = operands.iter().any(|o| {
                        is_decimal_expr(o, data_items)
                            || decimal_literal_parts(o).is_some()
                            || expr_is_scaled_display_numeric(o, data_items)
                    });
                    if target_is_decimal {
                        if has_size_error || rounded {
                            out.push_str(&format!("{pad}{{ "));
                            out.push_str(&decimal_init_statement("_sr", Some(target), data_items));
                            for (op_idx, op) in operands.iter().enumerate() {
                                let tmp = format!("_so{op_idx}");
                                out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                                out.push_str(&decimal_subtract_exact_statement("_sr", &tmp));
                            }
                            if let Some((target_size, target_scale, _)) =
                                display_numeric_c_expr_metadata(&c_target, data_items)
                            {
                                out.push_str(&decimal_rescale_to_scale_statement(
                                    "_sr",
                                    target_scale,
                                    rounded,
                                ));
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                if has_size_error {
                                    if let Some(max_val) = get_pic_max(
                                        target_name.map_or("", HirDataName::as_str),
                                        data_items,
                                    ) {
                                        out.push_str(&format!(
                                            "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); }} "
                                        ));
                                    } else {
                                        out.push_str(&format!(
                                            "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); "
                                        ));
                                    }
                                } else {
                                    out.push_str(&format!(
                                        "cobol_store_numeric_display(_result, {c_target_ptr}, {target_size}); "
                                    ));
                                }
                            } else {
                                if let Some(target_scale) = decimal_expr_scale(target, data_items) {
                                    out.push_str(&decimal_rescale_to_scale_statement(
                                        "_sr",
                                        target_scale,
                                        rounded,
                                    ));
                                    if has_size_error {
                                        if let Some(max_val) = get_pic_max(
                                            target_name.map_or("", HirDataName::as_str),
                                            data_items,
                                        ) {
                                            out.push_str(&format!(
                                                "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ {c_target}.value = _result; {c_target}.scale = {target_scale}; }} "
                                            ));
                                        } else {
                                            out.push_str(&format!(
                                                "{c_target}.value = _result; {c_target}.scale = {target_scale}; "
                                            ));
                                        }
                                    } else {
                                        out.push_str(&format!(
                                            "{c_target}.value = _result; {c_target}.scale = {target_scale}; "
                                        ));
                                    }
                                } else {
                                    out.push_str(&decimal_rescale_to_target_statement(
                                        "_sr", &c_target, rounded,
                                    ));
                                    if has_size_error {
                                        if let Some(max_val) = get_pic_max(
                                            target_name.map_or("", HirDataName::as_str),
                                            data_items,
                                        ) {
                                            out.push_str(&format!(
                                                "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ {c_target}.value = _result; }} "
                                            ));
                                        } else {
                                            out.push_str(&format!("{c_target}.value = _result; "));
                                        }
                                    } else {
                                        out.push_str(&format!("{c_target}.value = _result; "));
                                    }
                                }
                            }
                            out.push_str("}\n");
                        } else {
                            for op in operands {
                                if !emit_fast_decimal_sub_assign(
                                    out, &c_target, target, op, data_items, &pad,
                                ) {
                                    emit_decimal_arith(
                                        out,
                                        &c_target,
                                        op,
                                        "cobol_decimal_sub",
                                        data_items,
                                        &pad,
                                    );
                                }
                            }
                        }
                    } else if uses_decimal {
                        out.push_str(&format!("{pad}{{ "));
                        out.push_str(&decimal_init_statement("_sr", Some(target), data_items));
                        for (op_idx, op) in operands.iter().enumerate() {
                            let tmp = format!("_so{op_idx}");
                            out.push_str(&decimal_init_statement(&tmp, Some(op), data_items));
                            out.push_str(&decimal_subtract_exact_statement("_sr", &tmp));
                        }
                        out.push_str("int64_t _result = _sr.value; ");
                        out.push_str(&format!("int32_t _target_adjust = {scale_adjustment}; "));
                        if rounded {
                            out.push_str(
                                "int32_t _reduce = _sr.scale + ((_target_adjust > 0) ? _target_adjust : 0); \
                                 if (_reduce > 0) { \
                                     int64_t _factor = 1; \
                                     for (int32_t _i = 0; _i < _reduce; _i++) _factor *= 10; \
                                     _result = (_sr.value >= 0) ? ((_sr.value + (_factor / 2)) / _factor) : ((_sr.value - (_factor / 2)) / _factor); \
                                     if (_target_adjust > 0) { \
                                         for (int32_t _i = 0; _i < _target_adjust; _i++) _result *= 10; \
                                     } \
                                 } else if (_reduce < 0) { \
                                     for (int32_t _i = 0; _i < -_reduce; _i++) _result *= 10; \
                                 } ",
                            );
                        } else {
                            out.push_str(
                                "int32_t _reduce = _sr.scale + ((_target_adjust > 0) ? _target_adjust : 0); \
                                 if (_reduce > 0) { \
                                     int64_t _factor = 1; \
                                     for (int32_t _i = 0; _i < _reduce; _i++) _factor *= 10; \
                                     _result = _sr.value / _factor; \
                                     if (_target_adjust > 0) { \
                                         for (int32_t _i = 0; _i < _target_adjust; _i++) _result *= 10; \
                                     } \
                                 } else if (_reduce < 0) { \
                                     for (int32_t _i = 0; _i < -_reduce; _i++) _result *= 10; \
                                 } ",
                            );
                        }
                        if has_size_error {
                            if let Some(max_val) =
                                get_pic_max(target_name.map_or("", HirDataName::as_str), data_items)
                            {
                                out.push_str(&format!(
                                    "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                ));
                                emit_store_int(out, &c_target, "_result", data_items, "");
                                out.push_str("} ");
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, "");
                            }
                        } else {
                            emit_store_int(out, &c_target, "_result", data_items, "");
                        }
                        out.push_str("}\n");
                    } else if rounded && scale_adjustment > 0 {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        let current = apply_scale_adjustment_to_read(&c_target, scale_adjustment);
                        let result_expr = format!("{current} - ({sum_expr})");
                        out.push_str(&format!(
                            "{pad}{{ int64_t _raw = {result_expr}; \
                             int64_t _factor = {}; \
                             int64_t _result = (_raw >= 0) ? ((_raw + (_factor / 2)) / _factor) : ((_raw - (_factor / 2)) / _factor); \
                             _result *= _factor; ",
                            pow10_i64_literal(scale_adjustment as u32)
                        ));
                        if has_size_error {
                            if let Some(max_val) =
                                get_pic_max(target_name.map_or("", HirDataName::as_str), data_items)
                            {
                                out.push_str(&format!(
                                    "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                ));
                                emit_store_int(out, &c_target, "_result", data_items, "");
                                out.push_str("} ");
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, "");
                            }
                        } else {
                            emit_store_int(out, &c_target, "_result", data_items, "");
                        }
                        out.push_str("}\n");
                    } else {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            let current = if let Some(disp_size) =
                                grp_display_size(&c_target, data_items)
                            {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                format!("cobol_display_to_int64({c_target_const_ptr}, {disp_size})")
                            } else if find_data_item_by_c_name(&c_target, data_items)
                                .or_else(|| find_data_item(&c_target, data_items))
                                .is_some_and(|item| item.is_numeric_edited)
                            {
                                let size = find_data_item_size(&c_target, data_items);
                                format!("cobol_func_numval((const uint8_t*){c_target}, {size})")
                            } else {
                                target_name
                                    .and_then(|name| find_data_item_by_name(name, data_items))
                                    .or_else(|| find_data_item_by_c_name(&c_target, data_items))
                                    .or_else(|| find_data_item(&c_target, data_items))
                                    .filter(|item| item.scale_adjustment != 0)
                                    .map(|item| {
                                        apply_scale_adjustment_to_read(
                                            &c_target,
                                            item.scale_adjustment,
                                        )
                                    })
                                    .unwrap_or_else(|| c_target.clone())
                            };
                            let result_expr = format!("{current} - ({sum_expr})");
                            emit_size_checked_int_assignment(
                                out,
                                &c_target,
                                &result_expr,
                                target_name.map_or("", HirDataName::as_str),
                                data_items,
                                &pad,
                            );
                        } else {
                            emit_store_int_op(
                                out,
                                &c_target,
                                "-",
                                &format!("({sum_expr})"),
                                data_items,
                                &pad,
                            );
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let cond = emit_condition(condition, data_items);
            out.push_str(&format!("{pad}if ({cond}) {{\n"));
            for s in then_body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            if !else_body.is_empty() {
                out.push_str(&format!("{pad}}} else {{\n"));
                for s in else_body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                    );
                }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Perform { kind, .. } => {
            emit_perform(
                out,
                kind,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                current_paragraph,
                indent,
            );
        }
        HirStatement::Multiply {
            operand,
            by,
            by_rounded,
            giving,
            giving_rounded,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // MULTIPLY A BY B GIVING C [D ...]:  C = A * B
                let op_is_dec = is_decimal_expr(operand, data_items)
                    || decimal_literal_parts(operand).is_some_and(|(_, scale)| scale > 0)
                    || expr_is_scaled_display_numeric(operand, data_items);
                let by_is_dec = by.first().is_some_and(|b| {
                    is_decimal_expr(b, data_items)
                        || decimal_literal_parts(b).is_some_and(|(_, scale)| scale > 0)
                        || expr_is_scaled_display_numeric(b, data_items)
                });
                let any_src_decimal = op_is_dec || by_is_dec;
                let c_operand_int = emit_int_compatible_expr(operand, data_items);
                let first_by_int = by
                    .first()
                    .map(|b| emit_int_compatible_expr(b, data_items))
                    .unwrap_or_default();
                for (idx, target) in giving.iter().enumerate() {
                    if has_size_error {
                        out.push_str(&format!("{pad}if (!_size_error) {{\n"));
                    }
                    let rounded = giving_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_name
                        .and_then(|name| find_data_item_by_name(name, data_items))
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal || any_src_decimal {
                        if !has_size_error
                            && target_is_decimal
                            && emit_fast_decimal_multiply_giving(
                                out,
                                &c_target,
                                target,
                                operand,
                                by.first(),
                                rounded,
                                data_items,
                                &pad,
                            )
                        {
                            continue;
                        }
                        // Decimal path: convert operands through the shared initializer so DISPLAY
                        // numerics keep their PIC scale instead of being treated as plain integers.
                        let init_a = decimal_init_statement("_ma", Some(operand), data_items);
                        let init_b = decimal_init_statement("_mb", by.first(), data_items);
                        out.push_str(&format!("{pad}{{ {init_a} {init_b} "));
                        if rounded || has_size_error {
                            out.push_str(
                                "CobolDecimal _mr = { .value = (int64_t)((__int128)_ma.value * (__int128)_mb.value), \
                                 .scale = _ma.scale + _mb.scale, .size = _ma.size + _mb.size, \
                                 .is_signed = _ma.is_signed || _mb.is_signed }; ",
                            );
                        } else {
                            out.push_str("CobolDecimal _mr; cobol_decimal_mul(&_ma, &_mb, &_mr); ");
                        }
                        if target_is_decimal {
                            let exact_rescale = |rounded: bool| {
                                let rounding = if rounded {
                                    " _result = (_mr.value >= 0 ? ((_mr.value + (_factor / 2)) / _factor) : ((_mr.value - (_factor / 2)) / _factor)); "
                                } else {
                                    " _result = _mr.value / _factor; "
                                };
                                format!(
                                    "int64_t _result = 0; \
                                     if (_mr.scale > {c_target}.scale) {{ \
                                         int64_t _factor = 1; \
                                         for (uint32_t _i = 0; _i < _mr.scale - {c_target}.scale; _i++) _factor *= 10; \
                                         {rounding}\
                                     }} else {{ \
                                         int64_t _factor = 1; \
                                         for (uint32_t _i = 0; _i < {c_target}.scale - _mr.scale; _i++) _factor *= 10; \
                                         _result = _mr.value * _factor; \
                                     }} "
                                )
                            };
                            if has_size_error {
                                let target_name_str = target_name.map_or("", HirDataName::as_str);
                                out.push_str(&exact_rescale(rounded));
                                if let Some(max_val) = get_pic_max(target_name_str, data_items) {
                                    out.push_str(&format!(
                                        "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                         else {{ {c_target}.value = _result; }} }}\n"
                                    ));
                                } else {
                                    out.push_str(&format!("{c_target}.value = _result; }}\n"));
                                }
                            } else {
                                out.push_str(&exact_rescale(rounded));
                                out.push_str(&format!("{c_target}.value = _result; }}\n"));
                            }
                        } else if let Some(item) = find_data_item_by_c_name(&c_target, data_items)
                            .or_else(|| find_data_item(&c_target, data_items))
                            .filter(|item| item.is_numeric_edited)
                        {
                            emit_store_decimal_to_numeric_edited(
                                out, &c_target, "_mr", item, rounded, &pad,
                            );
                            out.push_str("}\n");
                        } else if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            let store_expr = if rounded {
                                "llround(cobol_decimal_to_double(&_mr))".to_string()
                            } else {
                                "cobol_decimal_to_int64(&_mr)".to_string()
                            };
                            if has_size_error {
                                let target_name_str = target_name.map_or("", HirDataName::as_str);
                                if let Some(max_val) = get_pic_max(target_name_str, data_items) {
                                    out.push_str(&format!(
                                        "int64_t _result = {store_expr}; \
                                         if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                         else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size}); }} }}\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "cobol_store_numeric_display(\
                                         {store_expr}, \
                                         {c_target_ptr}, {disp_size}); }}\n"
                                    ));
                                }
                            } else {
                                out.push_str(&format!(
                                    "cobol_store_numeric_display(\
                                     {store_expr}, \
                                     {c_target_ptr}, {disp_size}); }}\n"
                                ));
                            }
                        } else {
                            let store_expr = if rounded {
                                "llround(cobol_decimal_to_double(&_mr))"
                            } else {
                                "cobol_decimal_to_int64(&_mr)"
                            };
                            if has_size_error {
                                let target_name_str = target_name.map_or("", HirDataName::as_str);
                                if let Some(max_val) = get_pic_max(target_name_str, data_items) {
                                    out.push_str(&format!(
                                        "int64_t _result = {store_expr}; \
                                         if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                    ));
                                    emit_store_int(out, &c_target, "_result", data_items, "");
                                    out.push_str("} ");
                                } else {
                                    emit_store_int(out, &c_target, store_expr, data_items, "");
                                }
                            } else {
                                emit_store_int(out, &c_target, store_expr, data_items, "");
                            }
                            out.push_str("}\n");
                        }
                    } else {
                        let mul_expr = format!("{first_by_int} * {c_operand_int}");
                        if has_size_error {
                            emit_size_checked_int_assignment(
                                out,
                                &c_target,
                                &mul_expr,
                                target_name.map_or("", HirDataName::as_str),
                                data_items,
                                &pad,
                            );
                        } else {
                            emit_store_int(out, &c_target, &mul_expr, data_items, &pad);
                        }
                    }
                    if has_size_error {
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            } else {
                for (idx, target) in by.iter().enumerate() {
                    let rounded = by_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_name
                        .and_then(|name| find_data_item_by_name(name, data_items))
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    let display_target = display_numeric_c_expr_metadata(&c_target, data_items);
                    if let (true, Some((target_size, target_scale, _))) =
                        (target_is_decimal, display_target)
                    {
                        out.push_str(&format!("{pad}{{ "));
                        out.push_str(&decimal_init_statement("_mt", Some(target), data_items));
                        out.push_str(&decimal_init_statement("_mo", Some(operand), data_items));
                        out.push_str(
                            "CobolDecimal _mr = { \
                                 .value = (int64_t)((__int128)_mt.value * (__int128)_mo.value), \
                                 .scale = _mt.scale + _mo.scale, \
                                 .size = _mt.size + _mo.size, \
                                 .is_signed = _mt.is_signed || _mo.is_signed }; ",
                        );
                        out.push_str(&decimal_rescale_to_scale_statement(
                            "_mr",
                            target_scale,
                            rounded,
                        ));
                        if has_size_error {
                            if let Some(max_val) =
                                get_pic_max(target_name.map_or("", HirDataName::as_str), data_items)
                            {
                                out.push_str(&format!(
                                    "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                ));
                                emit_store_display_numeric(
                                    out,
                                    "",
                                    "_result",
                                    &c_target,
                                    target_size,
                                    data_items,
                                );
                                out.push_str("} ");
                            } else {
                                emit_store_display_numeric(
                                    out,
                                    "",
                                    "_result",
                                    &c_target,
                                    target_size,
                                    data_items,
                                );
                            }
                        } else {
                            emit_store_display_numeric(
                                out,
                                "",
                                "_result",
                                &c_target,
                                target_size,
                                data_items,
                            );
                        }
                        out.push_str("}\n");
                    } else if target_is_decimal {
                        if has_size_error {
                            let target_scale = decimal_expr_scale(target, data_items).unwrap_or(0);
                            let target_name_str = target_name.map_or("", HirDataName::as_str);
                            let max_val = get_pic_max(target_name_str, data_items);
                            out.push_str(&format!("{pad}{{ "));
                            out.push_str(&decimal_init_statement("_mt", Some(target), data_items));
                            out.push_str(&decimal_init_statement("_mo", Some(operand), data_items));
                            out.push_str(
                                "CobolDecimal _mr = { \
                                     .value = (int64_t)((__int128)_mt.value * (__int128)_mo.value), \
                                     .scale = _mt.scale + _mo.scale, \
                                     .size = _mt.size + _mo.size, \
                                     .is_signed = _mt.is_signed || _mo.is_signed }; ",
                            );
                            out.push_str(&decimal_rescale_to_scale_statement(
                                "_mr",
                                target_scale,
                                rounded,
                            ));
                            if let Some(max_val) = max_val {
                                out.push_str(&format!(
                                    "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
                                ));
                                out.push_str(&format!(
                                    "{c_target}.value = _result; {c_target}.scale = {target_scale}; }} "
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{c_target}.value = _result; {c_target}.scale = {target_scale}; "
                                ));
                            }
                            out.push_str("}\n");
                        } else if rounded {
                            emit_rounded_decimal_multiply_by(
                                out, &c_target, operand, data_items, &pad,
                            );
                        } else {
                            emit_decimal_arith(
                                out,
                                &c_target,
                                operand,
                                "cobol_decimal_mul",
                                data_items,
                                &pad,
                            );
                        }
                    } else if is_decimal_expr(operand, data_items)
                        || expr_is_scaled_display_numeric(operand, data_items)
                        || decimal_literal_parts(operand).is_some_and(|(_, scale)| scale > 0)
                    {
                        // int64 target *= decimal operand. Initialize through the shared decimal
                        // path so DISPLAY numerics with V/P keep their implied scale.
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            let target_name_str = target_name.map_or("", HirDataName::as_str);
                            let max_val = get_pic_max(target_name_str, data_items);
                            out.push_str(&format!(
                                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size});\n"
                            ));
                            out.push_str(&decimal_init_statement(
                                "_mop",
                                Some(operand),
                                data_items,
                            ));
                            emit_scaled_decimal_multiply_result(
                                out, "_prev", "_mop", rounded, &pad,
                            );
                            if has_size_error {
                                if let Some(max_val) = max_val {
                                    out.push_str(&format!(
                                        "{pad}if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                         else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size}); }}\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "{pad}cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size});\n"
                                    ));
                                }
                            } else {
                                out.push_str(&format!(
                                    "{pad}cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size});\n"
                                ));
                            }
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            let current_item = target_name
                                .and_then(|name| find_data_item_by_name(name, data_items))
                                .or_else(|| find_data_item_by_c_name(&c_target, data_items))
                                .or_else(|| find_data_item(&c_target, data_items));
                            let current = if current_item.is_some_and(|item| item.is_numeric_edited)
                            {
                                let size = find_data_item_size(&c_target, data_items);
                                format!("cobol_func_numval((const uint8_t*){c_target}, {size})")
                            } else if let Some(item) =
                                current_item.filter(|item| item.scale_adjustment != 0)
                            {
                                apply_scale_adjustment_to_read(&c_target, item.scale_adjustment)
                            } else {
                                c_target.clone()
                            };
                            out.push_str(&format!("{pad}{{ int64_t _prev = {current};\n"));
                            out.push_str(&decimal_init_statement(
                                "_mop",
                                Some(operand),
                                data_items,
                            ));
                            emit_scaled_decimal_multiply_result(
                                out, "_prev", "_mop", rounded, &pad,
                            );
                            if has_size_error {
                                if let Some(max_val) = get_pic_max(
                                    target_name.map_or("", HirDataName::as_str),
                                    data_items,
                                ) {
                                    out.push_str(&format!(
                                        "{pad}if (llabs(_result) > {max_val}) {{ _size_error = 1; "
                                    ));
                                    emit_store_int(out, &c_target, "_prev", data_items, "");
                                    out.push_str("} else { ");
                                    emit_store_int(out, &c_target, "_result", data_items, "");
                                    out.push_str("}\n");
                                } else {
                                    emit_store_int(out, &c_target, "_result", data_items, &pad);
                                }
                            } else {
                                emit_store_int(out, &c_target, "_result", data_items, &pad);
                            }
                            out.push_str(&format!("{pad}}}\n"));
                        }
                    } else {
                        let c_operand = emit_int_compatible_expr(operand, data_items);
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                let target_name_str = target_name.map_or("", HirDataName::as_str);
                                let max_val = get_pic_max(target_name_str, data_items);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                let result_expr = if rounded {
                                    format!("llround((double)(_prev * ({c_operand})))")
                                } else {
                                    format!("_prev * ({c_operand})")
                                };
                                out.push_str(&format!("{pad}int64_t _result = {result_expr};\n"));
                                if let Some(max_val) = max_val {
                                    out.push_str(&format!(
                                        "{pad}if (llabs(_result) > {max_val}) {{ _size_error = 1; }} \
                                         else {{ cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size}); }}\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "{pad}cobol_store_numeric_display(_result, {c_target_ptr}, {disp_size});\n"
                                    ));
                                }
                            } else {
                                let current = if find_data_item_by_c_name(&c_target, data_items)
                                    .or_else(|| find_data_item(&c_target, data_items))
                                    .is_some_and(|item| item.is_numeric_edited)
                                {
                                    let size = find_data_item_size(&c_target, data_items);
                                    format!("cobol_func_numval((const uint8_t*){c_target}, {size})")
                                } else {
                                    c_target.clone()
                                };
                                let result_expr = if rounded {
                                    format!("llround((double)({current} * ({c_operand})))")
                                } else {
                                    format!("{current} * ({c_operand})")
                                };
                                emit_size_checked_int_assignment(
                                    out,
                                    &c_target,
                                    &result_expr,
                                    target_name.map_or("", HirDataName::as_str),
                                    data_items,
                                    &pad,
                                );
                                continue;
                            }
                            emit_integer_overflow_check(
                                out,
                                target_name.map_or("", HirDataName::as_str),
                                &c_target,
                                data_items,
                                &pad,
                            );
                            out.push_str(&format!("{pad}}}\n"));
                        } else if rounded {
                            let current = if let Some(disp_size) =
                                grp_display_size(&c_target, data_items)
                            {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                format!("cobol_display_to_int64({c_target_const_ptr}, {disp_size})")
                            } else {
                                c_target.clone()
                            };
                            let product = format!("llround((double)(({current}) * ({c_operand})))");
                            emit_store_int(out, &c_target, &product, data_items, &pad);
                        } else {
                            emit_store_int_op(out, &c_target, "*", &c_operand, data_items, &pad);
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Divide {
            operand,
            into,
            into_rounded,
            giving,
            giving_rounded,
            remainder,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            let op_is_dec = is_decimal_expr(operand, data_items);
            let into_is_dec = into.first().is_some_and(|i| is_decimal_expr(i, data_items));
            let any_src_decimal = op_is_dec || into_is_dec;
            let c_operand = emit_expr(operand);
            let c_operand_int = emit_int_compatible_expr(operand, data_items);
            if !giving.is_empty() {
                // DIVIDE A INTO B GIVING C: C = B / A
                let first_into_int = into
                    .first()
                    .map(|i| emit_int_compatible_expr(i, data_items))
                    .unwrap_or_default();
                out.push_str(&format!("{pad}{{ "));
                out.push_str(&decimal_init_statement(
                    "_dg_into",
                    into.first(),
                    data_items,
                ));
                out.push_str(&decimal_init_statement(
                    "_dg_operand",
                    Some(operand),
                    data_items,
                ));
                out.push_str(&format!(
                    "int64_t _dg_into_int = ({first_into_int}); int64_t _dg_operand_int = ({c_operand_int});\n"
                ));
                for (idx, target) in giving.iter().enumerate() {
                    let rounded = giving_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_item =
                        target_name.and_then(|name| find_data_item_by_name(name, data_items));
                    let target_is_decimal =
                        target_item.is_some_and(|i| needs_decimal(&i.data_type));
                    if (target_is_decimal || any_src_decimal)
                        && display_numeric_c_expr_metadata(&c_target, data_items).is_some()
                    {
                        let init_a = "CobolDecimal _da = _dg_into; ";
                        let init_b = "CobolDecimal _db = _dg_operand; ";
                        let max_val =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items));
                        out.push_str(&emit_decimal_divide_to_display_statement(
                            &pad,
                            &c_target,
                            init_a,
                            init_b,
                            rounded,
                            has_size_error,
                            max_val,
                            data_items,
                        ));
                    } else if target_is_decimal {
                        let init_a = "CobolDecimal _da = _dg_into; ";
                        let init_b = "CobolDecimal _db = _dg_operand; ";
                        let max_val =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items));
                        out.push_str(&emit_decimal_divide_to_target_statement(
                            &pad,
                            &c_target,
                            init_a,
                            init_b,
                            rounded,
                            has_size_error,
                            max_val,
                        ));
                    } else if let Some(item) = target_item.filter(|item| item.is_numeric_edited) {
                        let init_a = "CobolDecimal _da = _dg_into; ";
                        let init_b = "CobolDecimal _db = _dg_operand; ";
                        let max_val = item.picture.as_deref().and_then(numeric_edited_integer_max);
                        out.push_str(&emit_decimal_divide_to_numeric_edited_statement(
                            &pad,
                            &c_target,
                            item,
                            init_a,
                            init_b,
                            rounded,
                            has_size_error,
                            max_val,
                        ));
                    } else if any_src_decimal {
                        let init_a = "CobolDecimal _da = _dg_into; ";
                        let init_b = "CobolDecimal _db = _dg_operand; ";
                        let max_val =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items));
                        emit_decimal_divide_to_int_target(
                            out,
                            &pad,
                            &c_target,
                            init_a,
                            init_b,
                            rounded,
                            has_size_error,
                            max_val,
                            data_items,
                        );
                    } else if has_size_error {
                        let div_expr = if rounded {
                            "llround((double)(_dg_into_int) / (double)(_dg_operand_int))"
                                .to_string()
                        } else {
                            "_dg_into_int / _dg_operand_int".to_string()
                        };
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size});\n"
                            ));
                            out.push_str(&format!(
                                "{pad}if (_dg_operand_int == 0) {{ _size_error = 1; }} \
                                 else {{ cobol_store_numeric_display({div_expr}, \
                                 {c_target_ptr}, {disp_size}); }}\n"
                            ));
                        } else {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!(
                                "{pad}if (_dg_operand_int == 0) {{ _size_error = 1; }} \
                                 else {{ {c_target} = {div_expr}; }}\n"
                            ));
                        }
                        emit_integer_overflow_check(
                            out,
                            target_name.map_or("", HirDataName::as_str),
                            &c_target,
                            data_items,
                            &pad,
                        );
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        let div_expr = if rounded {
                            "llround((double)(_dg_into_int) / (double)(_dg_operand_int))"
                                .to_string()
                        } else {
                            "_dg_into_int / _dg_operand_int".to_string()
                        };
                        emit_store_int(out, &c_target, &div_expr, data_items, &pad);
                    }
                    if let Some(rem) = remainder {
                        let c_rem = emit_expr(rem);
                        emit_divide_remainder_from_quotient(
                            out,
                            &pad,
                            target,
                            &c_target,
                            rounded,
                            rem,
                            &c_rem,
                            has_size_error,
                            data_items,
                        );
                    }
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                for (idx, target) in into.iter().enumerate() {
                    let rounded = into_rounded.get(idx).copied().unwrap_or(false);
                    let c_target = emit_expr(target);
                    let target_name = expr_data_name(target);
                    let target_is_decimal = target_name
                        .and_then(|name| find_data_item_by_name(name, data_items))
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal
                        && display_numeric_c_expr_metadata(&c_target, data_items).is_some()
                    {
                        let init_a = decimal_init_statement("_da", Some(target), data_items);
                        let init_b = decimal_init_statement("_db", Some(operand), data_items);
                        let max_val =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items));
                        out.push_str(&emit_decimal_divide_to_display_statement(
                            &pad,
                            &c_target,
                            &init_a,
                            &init_b,
                            rounded,
                            has_size_error,
                            max_val,
                            data_items,
                        ));
                    } else if target_is_decimal {
                        let init_a = format!("CobolDecimal _da = {c_target};");
                        let init_b = decimal_init_statement("_db", Some(operand), data_items);
                        let max_val =
                            target_name.and_then(|name| get_pic_max(name.as_str(), data_items));
                        out.push_str(&emit_decimal_divide_to_target_statement(
                            &pad,
                            &c_target,
                            &init_a,
                            &init_b,
                            rounded,
                            has_size_error,
                            max_val,
                        ));
                    } else if is_decimal_expr(operand, data_items) {
                        // int64 target /= CobolDecimal operand
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            if has_size_error {
                                out.push_str(&format!(
                                    "{pad}if ({c_operand}.value == 0) {{ _size_error = 1; }} \
                                     else {{ CobolDecimal _td; cobol_decimal_from_int(\
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}), 0, &_td); \
                                     cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                     cobol_store_numeric_display(\
                                     cobol_decimal_to_int64(&_td), \
                                     {c_target_ptr}, {disp_size}); }}\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}{{ CobolDecimal _td; cobol_decimal_from_int(\
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}), 0, &_td); \
                                     cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                     cobol_store_numeric_display(\
                                     cobol_decimal_to_int64(&_td), \
                                     {c_target_ptr}, {disp_size}); }}\n"
                                ));
                            }
                        } else {
                            if has_size_error {
                                out.push_str(&format!(
                                    "{pad}if ({c_operand}.value == 0) {{ _size_error = 1; }} \
                                     else {{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                                     cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                     {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}{{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                                     cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                     {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                                ));
                            }
                        }
                    } else {
                        if let Some(rem) = remainder {
                            let c_rem = emit_expr(rem);
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let rem_expr = format!(
                                    "cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}) % {c_operand_int}"
                                );
                                emit_store_int(out, &c_rem, &rem_expr, data_items, &pad);
                            } else {
                                let rem_expr = format!("{c_target} % {c_operand_int}");
                                emit_store_int(out, &c_rem, &rem_expr, data_items, &pad);
                            }
                        }
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                                     else {{ cobol_store_numeric_display(\
                                     {maybe_rounded}, \
                                     {c_target_ptr}, {disp_size}); }}\n"
                                ,
                                maybe_rounded = if rounded {
                                    format!(
                                        "llround((double)cobol_display_to_int64({c_target_const_ptr}, {disp_size}) / (double)({c_operand_int}))"
                                    )
                                } else {
                                    format!(
                                        "cobol_display_to_int64({c_target_const_ptr}, {disp_size}) / {c_operand_int}"
                                    )
                                }));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                let divide_value = if rounded {
                                    format!(
                                        "llround((double)({c_target}) / (double)({c_operand_int}))"
                                    )
                                } else {
                                    format!("{c_target} / {c_operand_int}")
                                };
                                out.push_str(&format!(
                                    "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                                     else {{ {c_target} = {divide_value}; }}\n"
                                ));
                            }
                            emit_integer_overflow_check(
                                out,
                                target_name.map_or("", HirDataName::as_str),
                                &c_target,
                                data_items,
                                &pad,
                            );
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            if rounded {
                                let current = if let Some(disp_size) =
                                    grp_display_size(&c_target, data_items)
                                {
                                    let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                    format!(
                                        "cobol_display_to_int64({c_target_const_ptr}, {disp_size})"
                                    )
                                } else {
                                    c_target.clone()
                                };
                                let result_expr = format!(
                                    "llround((double)({current}) / (double)({c_operand_int}))"
                                );
                                emit_store_int(out, &c_target, &result_expr, data_items, &pad);
                            } else {
                                emit_store_int_op(
                                    out,
                                    &c_target,
                                    "/",
                                    &c_operand_int,
                                    data_items,
                                    &pad,
                                );
                            }
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Call {
            program,
            params,
            on_exception,
            not_on_exception,
            ..
        } => {
            // Distinguish static CALL (literal string / known symbol) from
            // dynamic CALL (identifier expression that resolves at runtime).
            let dynamic_call_name = expr_data_name(program)
                .and_then(|name| find_data_item_by_name(name, data_items).map(|_| name))
                .map(|_| emit_comm_arg(program, data_items));
            let (prog_name, is_dynamic) = match program {
                HirExpr::Literal(HirLiteral::String(s)) => (sanitize_name(s), false),
                HirExpr::Variable(name) => {
                    let sname = data_name_to_c_name(name);
                    (sname, dynamic_call_name.is_some())
                }
                _ => (emit_expr(program), dynamic_call_name.is_some()),
            };
            let has_exception_handlers = !on_exception.is_empty() || !not_on_exception.is_empty();
            out.push_str(&format!("{pad}/* CALL {prog_name} */\n"));
            if has_exception_handlers {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    volatile int _call_failed = 0;\n"));
            }
            let inner_pad = if has_exception_handlers {
                format!("{pad}    ")
            } else {
                pad.to_string()
            };
            if is_dynamic {
                // Dynamic CALL: resolve function at runtime via dlsym.
                // The identifier expression contains the program name bytes.
                let (call_name_ptr, call_name_len) = dynamic_call_name
                    .clone()
                    .unwrap_or_else(|| ("NULL".to_string(), "0".to_string()));
                let param_count = params.len();
                if param_count == 0 {
                    out.push_str(&format!("{inner_pad}{{\n"));
                    out.push_str(&format!(
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({call_name_ptr}, {call_name_len}, _name, sizeof(_name));\n"
                    ));
                    let nested_names = with_active_context(|ctx| ctx.nested_program_names());
                    let has_nested_resolution = !nested_names.is_empty();
                    if has_nested_resolution {
                        out.push_str(&format!("{inner_pad}    int _resolved = 0;\n"));
                        for nested_name in &nested_names {
                            out.push_str(&format!(
                                "{inner_pad}    if (!_resolved && strcmp(_name, \"{nested_name}\") == 0) {{ _resolved = 1; jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {nested_name}(); cobol_call_leave(); }} }}\n"
                            ));
                        }
                    }
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)(void) = {}(void(*)(void))dlsym(RTLD_DEFAULT, _name);\n",
                        if has_nested_resolution { "_resolved ? NULL : " } else { "" }
                    ));
                    if has_exception_handlers {
                        if has_nested_resolution {
                            out.push_str(&format!(
                                "{inner_pad}    if (!_resolved && _fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }} else if (!_resolved) {{ _call_failed = 1; }}\n"
                            ));
                        } else {
                            out.push_str(&format!(
                                "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                            ));
                        }
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }}\n"
                        ));
                    }
                    out.push_str(&format!("{inner_pad}}}\n"));
                } else {
                    // Build param values
                    let mut param_values = Vec::new();
                    let mut content_copies = Vec::new();
                    for (i, p) in params.iter().enumerate() {
                        let arg = emit_expr(&p.expr);
                        match p.mode {
                            cobol_hir::HirParamMode::ByReference => {
                                param_values.push(format!("&{arg}"));
                            }
                            cobol_hir::HirParamMode::ByValue => {
                                let arg_int = emit_int_compatible_expr(&p.expr, data_items);
                                param_values.push(format!("(int64_t){arg_int}"));
                            }
                            cobol_hir::HirParamMode::ByContent => {
                                let copy_var = format!("_content_copy_{i}");
                                content_copies.push(format!(
                                    "{inner_pad}typeof({arg}) {copy_var}; memcpy(&{copy_var}, &{arg}, sizeof({arg}));\n"
                                ));
                                param_values.push(format!("&{copy_var}"));
                            }
                        }
                    }
                    out.push_str(&format!("{inner_pad}{{\n"));
                    for copy in &content_copies {
                        out.push_str(copy);
                    }
                    let values_str = param_values.join(", ");
                    // Build typedef for the function pointer type
                    let void_ptrs: Vec<&str> = (0..param_count).map(|_| "void*").collect();
                    let types_str = void_ptrs.join(", ");
                    out.push_str(&format!(
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({call_name_ptr}, {call_name_len}, _name, sizeof(_name));\n"
                    ));
                    let nested_names = with_active_context(|ctx| ctx.nested_program_names());
                    let has_nested_resolution = !nested_names.is_empty();
                    if has_nested_resolution {
                        out.push_str(&format!("{inner_pad}    int _resolved = 0;\n"));
                        for nested_name in &nested_names {
                            out.push_str(&format!(
                                "{inner_pad}    if (!_resolved && strcmp(_name, \"{nested_name}\") == 0) {{ _resolved = 1; jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {nested_name}({values_str}); cobol_call_leave(); }} }}\n"
                            ));
                        }
                    }
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)({types_str}) = {}(void(*)({types_str}))dlsym(RTLD_DEFAULT, _name);\n",
                        if has_nested_resolution { "_resolved ? NULL : " } else { "" }
                    ));
                    if has_exception_handlers {
                        if has_nested_resolution {
                            out.push_str(&format!(
                                "{inner_pad}    if (!_resolved && _fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }} else if (!_resolved) {{ _call_failed = 1; }}\n"
                            ));
                        } else {
                            out.push_str(&format!(
                                "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                            ));
                        }
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }}\n"
                        ));
                    }
                    out.push_str(&format!("{inner_pad}}}\n"));
                }
            } else if params.is_empty() {
                let is_nested_call =
                    with_active_context(|ctx| ctx.is_nested_program_name(&prog_name));
                out.push_str(&format!("{inner_pad}{{\n"));
                if is_nested_call {
                    out.push_str(&format!(
                        "{inner_pad}    jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}(); cobol_call_leave(); }}\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)(void) = (void(*)(void))dlsym(RTLD_DEFAULT, \"{prog_name}\");\n"
                    ));
                    if has_exception_handlers {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }}\n"
                        ));
                    }
                }
                out.push_str(&format!("{inner_pad}}}\n"));
            } else {
                // Wrap in a block to scope _content_copy_* variables
                // and avoid redefinition when multiple CALLs in same scope.
                out.push_str(&format!("{inner_pad}{{\n"));
                let call_pad = format!("{inner_pad}    ");
                let is_nested_call =
                    with_active_context(|ctx| ctx.is_nested_program_name(&prog_name));
                // Build param types and values based on passing mode
                let mut param_types = Vec::new();
                let mut param_values = Vec::new();
                let mut content_copies = Vec::new();
                for (i, p) in params.iter().enumerate() {
                    let arg = emit_expr(&p.expr);
                    match p.mode {
                        cobol_hir::HirParamMode::ByReference => {
                            param_types.push("void*".to_string());
                            param_values.push(format!("&{arg}"));
                        }
                        cobol_hir::HirParamMode::ByValue => {
                            let arg_int = emit_int_compatible_expr(&p.expr, data_items);
                            param_types.push("int64_t".to_string());
                            param_values.push(format!("(int64_t){arg_int}"));
                        }
                        cobol_hir::HirParamMode::ByContent => {
                            // BY CONTENT: create a copy and pass address of the copy
                            let copy_var = format!("_content_copy_{i}");
                            content_copies
                                .push(format!("{call_pad}typeof({arg}) {copy_var}; memcpy(&{copy_var}, &{arg}, sizeof({arg}));\n"));
                            param_types.push("void*".to_string());
                            param_values.push(format!("&{copy_var}"));
                        }
                    }
                }
                for copy in &content_copies {
                    out.push_str(copy);
                }
                let types_str = param_types.join(", ");
                let values_str = param_values.join(", ");
                if is_nested_call {
                    out.push_str(&format!(
                        "{call_pad}jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}({values_str}); cobol_call_leave(); }}\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "{call_pad}void (*_fp)({types_str}) = (void(*)({types_str}))dlsym(RTLD_DEFAULT, \"{prog_name}\");\n"
                    ));
                    if has_exception_handlers {
                        out.push_str(&format!(
                            "{call_pad}if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{call_pad}if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }}\n"
                        ));
                    }
                }
                out.push_str(&format!("{inner_pad}}}\n"));
            }
            if has_exception_handlers {
                emit_on_exception(
                    out,
                    on_exception,
                    not_on_exception,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Open { entries, .. } => {
            for entry in entries {
                let c_name = sanitize_name(&entry.file_name);
                let mode_val = match entry.mode {
                    HirOpenMode::Input => 0,
                    HirOpenMode::Output => 1,
                    HirOpenMode::IoMode => 2,
                    HirOpenMode::Extend => 3,
                };
                let mode_comment = match entry.mode {
                    HirOpenMode::Input => "INPUT",
                    HirOpenMode::Output => "OUTPUT",
                    HirOpenMode::IoMode => "I-O",
                    HirOpenMode::Extend => "EXTEND",
                };
                // Determine record length from data items via FD record (default 80)
                let record_var = resolve_file_record(&c_name);
                let rec_len = find_record_len(&record_var, data_items);
                // Use ASSIGN TO path if available, otherwise fall back to file name
                let inherited_assign = ctx.file_assignment(&c_name);
                let file_path_str = if !entry.assign_to.is_empty() {
                    entry.assign_to.as_str()
                } else if let Some(assign) = inherited_assign {
                    assign
                } else {
                    entry.file_name.as_str()
                };
                let escaped_name = escape_c_string(file_path_str);
                let name_len = file_path_str.len();
                let inherited_org = ctx.file_organization(&c_name).unwrap_or(entry.organization);
                let org_val = if inherited_org == 1
                    && sort_record_needs_conversion(&record_var, data_items)
                {
                    0
                } else {
                    inherited_org
                };
                let is_optional = entry.optional || ctx.file_is_optional(&c_name);
                let access_val = entry.access_mode;
                out.push_str(&format!("{pad}/* OPEN {mode_comment} {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                let needs_open_status_block = has_fs
                    || has_declaratives
                    || ctx.file_is_variable_record(&c_name)
                    || has_linage_counter(data_items)
                    || (org_val == 3 && !entry.alternate_keys.is_empty());
                let open_call = if org_val == 3 {
                    if let Some(record_key) = &entry.record_key {
                        let record_var = resolve_file_record(&c_name);
                        let rec_key_c = sanitize_name(record_key);
                        if let Some((key_offset, key_len)) =
                            find_field_offset_and_size(record_key, &record_var, data_items)
                        {
                            if is_optional {
                                format!(
                                    "cobol_file_open_indexed_optional(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {access_val}, {mode_val}, {rec_len}, {key_offset}, {key_len}, 1)"
                                )
                            } else {
                                format!(
                                    "cobol_file_open_indexed(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {access_val}, {mode_val}, {rec_len}, {key_offset}, {key_len})"
                                )
                            }
                        } else {
                            format!(
                                "cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, {access_val}, {mode_val}, {rec_len}) /* fallback: unresolved key {rec_key_c} */"
                            )
                        }
                    } else {
                        format!(
                            "cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, {access_val}, {mode_val}, {rec_len})"
                        )
                    }
                } else {
                    format!(
                        "cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, {access_val}, {mode_val}, {rec_len})"
                    )
                };
                if needs_open_status_block {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    uint32_t _fs = {open_call};\n"));
                    out.push_str(&format!(
                        "{pad}    if (_fs == 0 || _fs == 5) FILE_MODE_{c_name} = \"{mode_comment}\";\n"
                    ));
                    if ctx.file_is_variable_record(&c_name) {
                        out.push_str(&format!(
                            "{pad}    if (_fs == 0 || _fs == 5) cobol_file_set_variable(FILE_ID_{c_name});\n"
                        ));
                    }
                    if has_linage_counter(data_items) {
                        out.push_str(&format!(
                            "{pad}    if (_fs == 0 || _fs == 5) LINAGE_COUNTER = 1;\n"
                        ));
                    }
                    if org_val == 3 && !entry.alternate_keys.is_empty() {
                        let record_var = resolve_file_record(&c_name);
                        for alt_key in &entry.alternate_keys {
                            let alt_key_c = sanitize_name(&alt_key.name);
                            if let Some((key_offset, key_len)) =
                                find_field_offset_and_size(&alt_key.name, &record_var, data_items)
                            {
                                let duplicates = if alt_key.duplicates { 1 } else { 0 };
                                out.push_str(&format!(
                                    "{pad}    if (_fs == 0 || _fs == 5) {{ uint32_t _alt_fs = cobol_file_add_alternate_index(FILE_ID_{c_name}, {key_offset}, {key_len}, {duplicates}); if (_alt_fs != 0) _fs = _alt_fs; }}\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}    /* unresolved alternate key {alt_key_c} */\n"
                                ));
                            }
                        }
                    }
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("\"{mode_comment}\""),
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}{open_call};\n"));
                    if ctx.file_is_variable_record(&c_name) {
                        out.push_str(&format!(
                            "{pad}cobol_file_set_variable(FILE_ID_{c_name});\n"
                        ));
                    }
                    if has_linage_counter(data_items) {
                        out.push_str(&format!("{pad}LINAGE_COUNTER = 1;\n"));
                    }
                    if org_val == 3 && !entry.alternate_keys.is_empty() {
                        let record_var = resolve_file_record(&c_name);
                        for alt_key in &entry.alternate_keys {
                            let alt_key_c = sanitize_name(&alt_key.name);
                            if let Some((key_offset, key_len)) =
                                find_field_offset_and_size(&alt_key.name, &record_var, data_items)
                            {
                                let duplicates = if alt_key.duplicates { 1 } else { 0 };
                                out.push_str(&format!(
                                    "{pad}cobol_file_add_alternate_index(FILE_ID_{c_name}, {key_offset}, {key_len}, {duplicates});\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}/* unresolved alternate key {alt_key_c} */\n"
                                ));
                            }
                        }
                    }
                }
                emit_debug_spaces_event(out, &pad, entry.file_name.as_str());
            }
        }
        HirStatement::Close {
            files,
            close_options,
            ..
        } => {
            for (idx, file) in files.iter().enumerate() {
                let c_name = sanitize_name(file);
                out.push_str(&format!("{pad}/* CLOSE {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                let close_option = close_options.get(idx).copied().flatten();
                let unsupported_reel_or_unit = matches!(
                    close_option,
                    Some(HirCloseOption::Reel | HirCloseOption::Unit)
                );
                let close_func = if matches!(close_option, Some(HirCloseOption::WithLock)) {
                    "cobol_file_close_with_lock"
                } else {
                    "cobol_file_close"
                };
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    if unsupported_reel_or_unit {
                        out.push_str(&format!("{pad}    uint32_t _fs = 7;\n"));
                    } else {
                        out.push_str(&format!(
                            "{pad}    uint32_t _fs = {close_func}(FILE_ID_{c_name});\n"
                        ));
                    }
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives && !unsupported_reel_or_unit,
                        &format!("FILE_MODE_{c_name}"),
                        &format!("{pad}    "),
                    );
                    if !unsupported_reel_or_unit {
                        out.push_str(&format!(
                            "{pad}    if (_fs == 0) FILE_MODE_{c_name} = \"\";\n"
                        ));
                    }
                    out.push_str(&format!("{pad}}}\n"));
                } else if unsupported_reel_or_unit {
                    out.push_str(&format!(
                        "{pad}/* CLOSE REEL/UNIT unsupported for {c_name}; file remains open */\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}{close_func}(FILE_ID_{c_name});\n"));
                }
                emit_debug_spaces_event(out, &pad, file.as_str());
            }
        }
        HirStatement::Read {
            file_name,
            is_next,
            into,
            key,
            at_end,
            not_at_end,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let (read_target, event_target_name) = (record_var.clone(), record_var.clone());
            let into_target = into.as_ref().map(|(into_var, into_subs)| {
                if into_subs.is_empty() {
                    let n = sanitize_name(into_var);
                    (n.clone(), n)
                } else {
                    let access =
                        emit_subscript_access(&HirDataName::simple(into_var.clone()), into_subs);
                    let n = sanitize_name(into_var);
                    (access, n)
                }
            });
            let rec_len = find_record_len(&record_var, data_items);
            let needs_read_conv = sort_record_needs_conversion(&read_target, data_items);
            let read_buffer = if needs_read_conv {
                "_file_flat".to_string()
            } else {
                format!("(uint8_t*)&{read_target}")
            };
            out.push_str(&format!("{pad}/* READ {c_name} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            if needs_read_conv {
                out.push_str(&format!("{pad}    uint8_t _file_flat[{rec_len}];\n"));
            }
            let effective_key = key.as_deref().or_else(|| {
                if !*is_next
                    && ctx.file_organization(&c_name) == Some(2)
                    && ctx.file_access_mode(&c_name) != Some(0)
                {
                    ctx.relative_key_for_file(&c_name)
                } else {
                    None
                }
            });
            if *is_next {
                out.push_str(&format!(
                    "{pad}    uint32_t _fs = cobol_file_read_next(FILE_ID_{c_name}, {read_buffer}, {rec_len});\n"
                ));
            } else if let Some(key_name) = effective_key {
                let resolved_key = resolve_record_key_item(key_name, &record_var, data_items);
                let c_key = resolved_key
                    .as_ref()
                    .map(|(c_key, _, _)| c_key.clone())
                    .unwrap_or_else(|| sanitize_name(key_name));
                if ctx.file_organization(&c_name) == Some(2) {
                    let rel_expr = emit_numeric_expr_for_var(&c_key, data_items);
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_read_relative(FILE_ID_{c_name}, (uint64_t)({rel_expr}), {read_buffer}, {rec_len});\n"
                    ));
                } else {
                    let key_size = resolved_key
                        .as_ref()
                        .map(|(_, size, _)| *size)
                        .unwrap_or_else(|| find_data_item_size(&c_key, data_items));
                    let is_key_group = resolved_key
                        .as_ref()
                        .map(|(_, _, is_group)| *is_group)
                        .unwrap_or_else(|| {
                            find_data_item(key_name, data_items)
                                .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }))
                        });
                    let addr_prefix = if is_key_group { "&" } else { "" };
                    let key_offset = if ctx.file_organization(&c_name) == Some(3) {
                        find_field_offset_and_size(key_name, &record_var, data_items)
                            .map(|(offset, _)| offset)
                            .unwrap_or(u32::MAX)
                    } else {
                        0
                    };
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_read_key(FILE_ID_{c_name}, (const uint8_t*){addr_prefix}{c_key}, {key_size}, {key_offset}, {read_buffer}, {rec_len});\n"
                    ));
                }
            } else {
                out.push_str(&format!(
                    "{pad}    uint32_t _fs = cobol_file_read_next(FILE_ID_{c_name}, {read_buffer}, {rec_len});\n"
                ));
            }
            if (effective_key.is_none() || *is_next) && ctx.file_organization(&c_name) == Some(2) {
                if let Some(relative_key) = ctx.relative_key_for_file(&c_name) {
                    let c_relative_key = sanitize_name(relative_key);
                    let key_digits = relative_key_integer_digits(&c_relative_key, data_items);
                    let key_max = relative_key_max_value(key_digits);
                    out.push_str(&format!(
                        "{pad}    if ((_fs == 0 || _fs == 2) && cobol_file_current_record(FILE_ID_{c_name}) > {key_max}ULL) _fs = 14;\n"
                    ));
                }
            }
            emit_file_status_update(
                out,
                &c_name,
                "_fs",
                fs_map,
                false,
                &format!("FILE_MODE_{c_name}"),
                &format!("{pad}    "),
            );
            let read_success_guard = "(_fs == 0 || _fs == 2)";
            if has_declaratives {
                let dispatch_fn =
                    with_active_context(|ctx| ctx.file_declarative_dispatch_fn().to_string());
                let at_end_guard = if at_end.is_empty() {
                    "0".to_string()
                } else {
                    "(_fs == 10)".to_string()
                };
                let invalid_key_guard = if invalid_key.is_empty() {
                    "0".to_string()
                } else {
                    "(_fs != 0 && _fs != 2)".to_string()
                };
                out.push_str(&format!(
                    "{pad}    if (!({read_success_guard} || {at_end_guard} || {invalid_key_guard})) {{\n"
                ));
                out.push_str(&format!(
                    "{pad}        {dispatch_fn}(\"{c_name}\", FILE_MODE_{c_name}, _fs);\n"
                ));
                out.push_str(&format!(
                    "{pad}        if (_goto_target) goto _goto_dispatch;\n"
                ));
                out.push_str(&format!("{pad}    }}\n"));
            }
            if needs_read_conv {
                out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                emit_sort_record_deserialize(
                    out,
                    &read_target,
                    data_items,
                    "_file_flat",
                    &format!("{pad}        "),
                );
                out.push_str(&format!("{pad}    }}\n"));
            }
            emit_debug_data_name_event(
                out,
                &format!("{pad}    "),
                file_name.as_str(),
                &event_target_name,
                data_items,
                Some(read_success_guard),
                false,
                true,
            );
            if let Some(depending) = ctx.variable_record_depending(&c_name) {
                out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                emit_store_int(
                    out,
                    depending,
                    &format!("(int64_t)cobol_file_current_record_length(FILE_ID_{c_name})"),
                    data_items,
                    &format!("{pad}        "),
                );
                out.push_str(&format!("{pad}    }}\n"));
            }
            if let Some((into_t, into_name)) = &into_target {
                let into_len = find_data_item_size(into_name, data_items);
                out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                if ctx.file_is_variable_record(&c_name) {
                    out.push_str(&format!(
                        "{pad}        uint32_t _actual_len = cobol_file_current_record_length(FILE_ID_{c_name});\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        uint32_t _copy_len = _actual_len < {into_len} ? _actual_len : {into_len};\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        memcpy(&{into_t}, &{read_target}, _copy_len);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        if ({into_len} > _copy_len) memset(((uint8_t*)&{into_t}) + _copy_len, ' ', {into_len} - _copy_len);\n"
                    ));
                } else {
                    let copy_len = rec_len.min(into_len);
                    out.push_str(&format!(
                        "{pad}        memcpy(&{into_t}, &{read_target}, {copy_len});\n"
                    ));
                    if into_len > copy_len {
                        out.push_str(&format!(
                            "{pad}        memset(((uint8_t*)&{into_t}) + {copy_len}, ' ', {});\n",
                            into_len - copy_len
                        ));
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            let same_record_peers = ctx.same_record_peers(&c_name);
            if !same_record_peers.is_empty() {
                out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                for peer_record in same_record_peers {
                    let peer_len = find_record_len(peer_record, data_items);
                    let copy_len = rec_len.min(peer_len);
                    out.push_str(&format!(
                        "{pad}        memcpy(&{peer_record}, &{read_target}, {copy_len});\n"
                    ));
                    if peer_len > copy_len {
                        out.push_str(&format!(
                            "{pad}        memset(((uint8_t*)&{peer_record}) + {copy_len}, ' ', {});\n",
                            peer_len - copy_len
                        ));
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            if (effective_key.is_none() || *is_next) && ctx.file_organization(&c_name) == Some(2) {
                if let Some(relative_key) = ctx.relative_key_for_file(&c_name) {
                    out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                    emit_store_int(
                        out,
                        relative_key,
                        &format!("(int64_t)cobol_file_current_record(FILE_ID_{c_name})"),
                        data_items,
                        &format!("{pad}        "),
                    );
                    out.push_str(&format!("{pad}    }}\n"));
                }
            }
            if !invalid_key.is_empty() || !not_invalid_key.is_empty() {
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_fs != 0 && _fs != 2) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if ({read_success_guard}) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
            }
            if !at_end.is_empty() || !not_at_end.is_empty() {
                out.push_str(&format!("{pad}    if (_fs == 10) {{\n"));
                out.push_str(&format!("{pad}        /* AT END */\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                if !not_at_end.is_empty() {
                    out.push_str(&format!("{pad}    }} else {{\n"));
                    out.push_str(&format!("{pad}        /* NOT AT END */\n"));
                    for s in not_at_end {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Write {
            record_name,
            file_name,
            from,
            advancing,
            invalid_key,
            not_invalid_key,
            at_eop,
            not_at_eop,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                ctx.file_for_record(&c_name)
                    .unwrap_or_else(|| c_name.clone())
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_data_item_size(&c_name, data_items);
            let write_len = variable_record_io_len_expr(ctx, &c_file, rec_len);
            let boundary_error = variable_record_boundary_error_expr(ctx, &c_file);
            let needs_write_conv = sort_record_needs_conversion(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* WRITE {c_name} */\n"));
            if let Some(from_expr) = from {
                let source_len = alphanumeric_expr_len_c_expr(from_expr, data_items)
                    .unwrap_or_else(|| rec_len.to_string());
                out.push_str(&format!("{pad}memset(&{c_name}, ' ', {rec_len});\n"));
                out.push_str(&format!(
                    "{pad}memcpy(&{c_name}, &{source}, ({source_len}) < {rec_len} ? ({source_len}) : {rec_len});\n"
                ));
            }
            emit_debug_data_name_event(
                out,
                &pad,
                record_name.as_str(),
                &c_name,
                data_items,
                None,
                true,
                true,
            );
            if needs_write_conv {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    uint8_t _file_flat[{rec_len}];\n"));
                emit_sort_record_serialize(
                    out,
                    &c_name,
                    data_items,
                    "_file_flat",
                    &format!("{pad}    "),
                );
            }
            let write_ptr = if needs_write_conv {
                "_file_flat".to_string()
            } else {
                format!("(const uint8_t*)&{c_name}")
            };
            let write_call = if ctx.file_organization(&c_file) == Some(2)
                && ctx.file_access_mode(&c_file) != Some(0)
            {
                if let Some(relative_key) = ctx.relative_key_for_file(&c_file) {
                    let rel_expr = emit_numeric_expr_for_var(relative_key, data_items);
                    format!(
                        "cobol_file_write_relative(FILE_ID_{c_file}, (uint64_t)({rel_expr}), {write_ptr}, {write_len})"
                    )
                } else {
                    format!("cobol_file_write(FILE_ID_{c_file}, {write_ptr}, {write_len})")
                }
            } else {
                format!("cobol_file_write(FILE_ID_{c_file}, {write_ptr}, {write_len})")
            };
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            if needs_rc {
                out.push_str(&format!("{pad}{{\n"));
                if let Some(boundary_error) = &boundary_error {
                    out.push_str(&format!(
                        "{pad}    uint32_t _wrc = ({boundary_error}) ? 44 : {write_call};\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}    uint32_t _wrc = {write_call};\n"));
                }
                let has_fs = fs_map.contains_key(&c_file);
                if has_fs {
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_wrc",
                        fs_map,
                        has_declaratives,
                        &format!("FILE_MODE_{c_file}"),
                        &format!("{pad}    "),
                    );
                }
                emit_successful_write_followups(
                    out,
                    advancing.as_ref(),
                    at_eop,
                    not_at_eop,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                    Some("_wrc == 0"),
                );
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_wrc != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_wrc == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                let has_fs = fs_map.contains_key(&c_file);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    if let Some(boundary_error) = &boundary_error {
                        out.push_str(&format!(
                            "{pad}    uint32_t _fs = ({boundary_error}) ? 44 : {write_call};\n"
                        ));
                    } else {
                        out.push_str(&format!("{pad}    uint32_t _fs = {write_call};\n"));
                    }
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("FILE_MODE_{c_file}"),
                        &format!("{pad}    "),
                    );
                    emit_successful_write_followups(
                        out,
                        advancing.as_ref(),
                        at_eop,
                        not_at_eop,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        Some("_fs == 0"),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else if has_declaratives {
                    out.push_str(&format!("{pad}{{\n"));
                    if let Some(boundary_error) = &boundary_error {
                        out.push_str(&format!(
                            "{pad}    uint32_t _fs = ({boundary_error}) ? 44 : {write_call};\n"
                        ));
                    } else {
                        out.push_str(&format!("{pad}    uint32_t _fs = {write_call};\n"));
                    }
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        true,
                        &format!("FILE_MODE_{c_file}"),
                        &format!("{pad}    "),
                    );
                    emit_successful_write_followups(
                        out,
                        advancing.as_ref(),
                        at_eop,
                        not_at_eop,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        Some("_fs == 0"),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else if let Some(boundary_error) = &boundary_error {
                    out.push_str(&format!("{pad}if (!({boundary_error})) {{\n"));
                    out.push_str(&format!("{pad}    {write_call};\n"));
                    emit_successful_write_followups(
                        out,
                        advancing.as_ref(),
                        at_eop,
                        not_at_eop,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        None,
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else if has_linage_counter(data_items)
                    || !at_eop.is_empty()
                    || !not_at_eop.is_empty()
                {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    {write_call};\n"));
                    emit_successful_write_followups(
                        out,
                        advancing.as_ref(),
                        at_eop,
                        not_at_eop,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        None,
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}{write_call};\n"));
                }
            }
            if needs_write_conv {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Rewrite {
            record_name,
            file_name,
            from,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                ctx.file_for_record(&c_name)
                    .unwrap_or_else(|| c_name.clone())
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_data_item_size(&c_name, data_items);
            let rewrite_len = variable_record_io_len_expr(ctx, &c_file, rec_len);
            let boundary_error = variable_record_boundary_error_expr(ctx, &c_file);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* REWRITE {c_name} */\n"));
            if let Some(from_expr) = from {
                let source_len = alphanumeric_expr_len_c_expr(from_expr, data_items)
                    .unwrap_or_else(|| rec_len.to_string());
                out.push_str(&format!("{pad}memset(&{c_name}, ' ', {rec_len});\n"));
                out.push_str(&format!(
                    "{pad}memcpy(&{c_name}, &{source}, ({source_len}) < {rec_len} ? ({source_len}) : {rec_len});\n"
                ));
            }
            emit_debug_data_name_event(
                out,
                &pad,
                record_name.as_str(),
                &c_name,
                data_items,
                None,
                true,
                true,
            );
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            let has_fs = fs_map.contains_key(&c_file);
            if needs_rc || has_fs {
                out.push_str(&format!("{pad}{{\n"));
                if let Some(boundary_error) = &boundary_error {
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = ({boundary_error}) ? 44 : cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{c_name}, {rewrite_len});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{c_name}, {rewrite_len});\n"
                    ));
                }
                if has_fs {
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("FILE_MODE_{c_file}"),
                        &format!("{pad}    "),
                    );
                }
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_fs != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_fs == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                if let Some(boundary_error) = &boundary_error {
                    out.push_str(&format!("{pad}if (!({boundary_error})) {{\n"));
                    out.push_str(&format!(
                        "{pad}    cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{c_name}, {rewrite_len});\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{c_name}, {rewrite_len});\n"
                    ));
                }
            }
        }
        HirStatement::Delete {
            file_name,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            out.push_str(&format!("{pad}/* DELETE {c_name} */\n"));
            let delete_call = if ctx.file_organization(&c_name) == Some(3) {
                let record_var = resolve_file_record(&c_name);
                let rec_len = find_record_len(&record_var, data_items);
                format!("cobol_file_delete_record(FILE_ID_{c_name}, (const uint8_t*)&{record_var}, {rec_len})")
            } else if ctx.file_organization(&c_name) == Some(2) {
                if let Some(relative_key) = ctx.relative_key_for_file(&c_name) {
                    let rel_expr = emit_numeric_expr_for_var(relative_key, data_items);
                    format!("cobol_file_delete_relative(FILE_ID_{c_name}, (uint64_t)({rel_expr}))")
                } else {
                    format!("cobol_file_delete(FILE_ID_{c_name})")
                }
            } else {
                format!("cobol_file_delete(FILE_ID_{c_name})")
            };
            let has_fs = fs_map.contains_key(&c_name);
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            if has_fs || needs_rc {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    uint32_t _fs = {delete_call};\n"));
                if has_fs {
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("FILE_MODE_{c_name}"),
                        &format!("{pad}    "),
                    );
                }
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_fs != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_fs == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                out.push_str(&format!("{pad}{delete_call};\n"));
            }
            emit_debug_spaces_event(out, &pad, file_name.as_str());
        }
        HirStatement::GoTo {
            targets,
            depending_on,
            ..
        } => {
            let in_body = with_active_context(|ctx| ctx.in_body_context());
            let alter_info = current_paragraph.and_then(|id| ctx.alterable_paragraph(id));
            if let Some(dep) = depending_on {
                let c_dep = emit_expr(dep);
                let dep_value = emit_int_compatible_expr(dep, data_items);
                let dep_name = expr_data_name(dep)
                    .map(|name| escape_c_string(name.as_str()))
                    .unwrap_or_else(|| escape_c_string(&c_dep));
                if should_emit_debug_events() {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    char _debug_ref_contents[81];\n"));
                    out.push_str(&format!(
                        "{pad}    snprintf(_debug_ref_contents, sizeof(_debug_ref_contents), \"%lld\", (long long){dep_value});\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    _set_debug_event(\"{dep_name}\", _debug_ref_contents, \"\");\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    _dispatch_debug_reference(\"{dep_name}\");\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                out.push_str(&format!("{pad}switch ((int)({dep_value})) {{\n"));
                for (i, target) in targets.iter().enumerate() {
                    out.push_str(&format!("{pad}    case {}:\n", i + 1));
                    emit_transfer_to_target(
                        out,
                        target,
                        paragraphs,
                        &format!("{pad}        "),
                        in_body,
                        current_paragraph,
                    );
                    out.push_str(&format!("{pad}        break;\n"));
                }
                out.push_str(&format!("{pad}    default: break;\n"));
                out.push_str(&format!("{pad}}}\n"));
            } else if let Some(target) = targets.first() {
                if let Some(info) = alter_info {
                    if info.default_target.as_ref() == Some(target) {
                        emit_alterable_goto_dispatch(
                            out,
                            &info,
                            paragraphs,
                            &pad,
                            in_body,
                            current_paragraph,
                        );
                    } else {
                        emit_transfer_to_target(
                            out,
                            target,
                            paragraphs,
                            &pad,
                            in_body,
                            current_paragraph,
                        );
                    }
                } else {
                    emit_transfer_to_target(
                        out,
                        target,
                        paragraphs,
                        &pad,
                        in_body,
                        current_paragraph,
                    );
                }
            } else {
                if let Some(info) = alter_info {
                    emit_alterable_goto_dispatch(
                        out,
                        &info,
                        paragraphs,
                        &pad,
                        in_body,
                        current_paragraph,
                    );
                } else {
                    // GO TO. (no target) - alterable GO TO without ALTER applied.
                    // Fall through to the next statement (no-op).
                    out.push_str(&format!("{pad}/* GO TO (no target - alterable) */\n"));
                }
            }
        }
        HirStatement::Alter { pairs, .. } => {
            for (from, to) in pairs {
                let from_name = escape_c_string(from.name());
                let to_name = escape_c_string(to.name());
                if let Some(info) = from
                    .paragraph_id()
                    .and_then(|id| ctx.alterable_paragraph(id))
                {
                    if let Some(target_id) = to.paragraph_id() {
                        out.push_str(&format!("{pad}{} = {};\n", info.dispatch_var, target_id.0));
                    }
                }
                if should_emit_debug_events() {
                    out.push_str(&format!(
                        "{pad}_set_debug_event(\"{from_name}\", \"{to_name}\", \"\");\n"
                    ));
                    out.push_str(&format!(
                        "{pad}_dispatch_debug_procedure(\"{from_name}\");\n"
                    ));
                }
            }
        }
        HirStatement::Initialize {
            targets, replacing, ..
        } => {
            for target in targets {
                let c_target = sanitize_name(target);
                emit_initialize_field(out, target, &c_target, data_items, replacing, &pad);
            }
        }
        HirStatement::Set { targets, value, .. } => {
            for target in targets {
                let target_name = expr_data_name(target);
                let c_target = emit_expr(target);
                let target_item =
                    target_name.and_then(|name| find_data_item_by_name(name, data_items));
                let target_is_decimal = target_item.is_some_and(|i| needs_decimal(&i.data_type));
                let target_is_alpha = target_item
                    .is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
                let target_is_group =
                    target_item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
                let c_tgt_base = target_name.map(data_name_to_c_name).unwrap_or_default();
                if target_is_decimal {
                    emit_assign_to_decimal(out, value, &c_target, data_items, &pad);
                } else if target_is_alpha {
                    let c_value = emit_int_compatible_expr(value, data_items);
                    let tgt_size = find_data_item_size(&c_tgt_base, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_numeric_to_display({c_value}, 0, \
                         (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if target_is_group {
                    let c_value = emit_int_compatible_expr(value, data_items);
                    let tgt_size = find_data_item_size(&c_tgt_base, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_numeric_to_display({c_value}, 0, \
                         (uint8_t*)&{c_target}, {tgt_size});\n"
                    ));
                } else {
                    let c_value = emit_int_compatible_expr(value, data_items);
                    if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                        let c_target_ptr = display_numeric_ptr(&c_target);
                        out.push_str(&format!(
                            "{pad}cobol_store_numeric_display({c_value}, \
                             {c_target_ptr}, {disp_size});\n"
                        ));
                    } else {
                        out.push_str(&format!("{pad}{c_target} = {c_value};\n"));
                    }
                }
            }
        }
        HirStatement::SetSwitchStatus { assignments, .. } => {
            for (target, value) in assignments {
                let c_target = sanitize_name(target);
                let c_value = if *value { "1" } else { "0" };
                out.push_str(&format!("{pad}{c_target} = {c_value};\n"));
            }
        }
        HirStatement::SetAddress { target, source, .. } => {
            let c_target = sanitize_name(target);
            let c_source = sanitize_name(source);
            out.push_str(&format!(
                "{pad}{c_target} = (void*){c_source}; /* SET ADDRESS OF */\n"
            ));
        }
        HirStatement::StringStmt {
            into,
            sources,
            pointer,
            on_overflow,
            not_on_overflow,
            ..
        } => {
            let c_into = sanitize_name(into);
            let into_size = find_data_item_size(&c_into, data_items);
            out.push_str(&format!("{pad}/* STRING INTO {c_into} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            let src_count = sources.len();
            // Emit source value and optional delimiter for each source
            for (i, src) in sources.iter().enumerate() {
                emit_string_source_value(out, &src.value, i, data_items, &pad);
                emit_string_source_delimiter(out, &src.delimiter, i, data_items, &pad);
            }
            // Build the CobolStringSource array
            out.push_str(&format!(
                "{pad}    CobolStringSource _sources[{src_count}];\n"
            ));
            for i in 0..src_count {
                out.push_str(&format!(
                    "{pad}    _sources[{i}].ptr = (const uint8_t*)_src_ptr_{i}; _sources[{i}].len = _src_len_{i}; _sources[{i}].delim_ptr = _delim_ptr_{i}; _sources[{i}].delim_len = _delim_len_{i};\n"
                ));
            }
            let into_ptr = c_ptr_expr(&c_into, data_items);
            if let Some(pointer) = pointer {
                let pointer_expr =
                    HirExpr::Variable(cobol_hir::HirDataName::simple(pointer.clone()));
                let pointer_value = emit_int_compatible_expr(&pointer_expr, data_items);
                out.push_str(&format!(
                    "{pad}    uint32_t _pointer = (uint32_t)({pointer_value});\n"
                ));
            } else {
                out.push_str(&format!("{pad}    uint32_t _pointer = 1;\n"));
            }
            out.push_str(&format!(
                "{pad}    int32_t _str_rc = cobol_string_concat(_sources, {src_count}, (uint8_t*){into_ptr}, {into_size}, &_pointer);\n"
            ));
            if let Some(pointer) = pointer {
                let c_pointer = sanitize_name(pointer);
                emit_store_int(
                    out,
                    &c_pointer,
                    "(int64_t)_pointer",
                    data_items,
                    &format!("{pad}    "),
                );
            }
            if !on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_str_rc != 0) {{\n"));
                for s in on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            if !not_on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_str_rc == 0) {{\n"));
                for s in not_on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::UnstringStmt {
            source,
            delimiters,
            into,
            pointer,
            tallying,
            on_overflow,
            not_on_overflow,
            ..
        } => {
            let c_source = sanitize_name(source);
            let source_expr = HirExpr::Variable(cobol_hir::HirDataName::simple(source.clone()));
            let src_size = alphanumeric_expr_len_expr(&source_expr, data_items)
                .unwrap_or_else(|| find_data_item_size(&c_source, data_items).to_string());
            let targets: Vec<_> = into
                .iter()
                .map(|target| sanitize_name(&target.target))
                .collect();
            let tgt_count = into.len();
            out.push_str(&format!(
                "{pad}/* UNSTRING {c_source} INTO {} */\n",
                targets.join(", ")
            ));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    CobolUnstringTarget _targets[{tgt_count}];\n"
            ));
            for (i, target) in into.iter().enumerate() {
                let tgt = sanitize_name(&target.target);
                let mut tgt_size = find_data_item_size(&tgt, data_items);
                let tgt_ptr = c_ptr_expr(&tgt, data_items);
                let target_kind = if with_active_context(|ctx| ctx.is_justified_name(&tgt)) {
                    1
                } else if is_numeric_item_c(&tgt, data_items)
                    && !with_active_context(|ctx| ctx.has_display_numeric(&tgt))
                {
                    if let Some(item) = find_data_item_by_c_name(&tgt, data_items) {
                        if let HirType::Numeric { size, .. } = item.data_type {
                            tgt_size = size;
                        }
                    }
                    2
                } else {
                    0
                };
                out.push_str(&format!(
                    "{pad}    _targets[{i}].ptr = (uint8_t*){tgt_ptr}; _targets[{i}].len = {tgt_size}; _targets[{i}].delimiter_ptr = NULL; _targets[{i}].delimiter_len = 0; _targets[{i}].count_ptr = NULL; _targets[{i}].kind = {target_kind};\n"
                ));
                if let Some(delimiter_in) = &target.delimiter_in {
                    let c_delim = sanitize_name(delimiter_in);
                    let delim_size = find_data_item_size(&c_delim, data_items);
                    let delim_ptr = c_ptr_expr(&c_delim, data_items);
                    out.push_str(&format!(
                        "{pad}    _targets[{i}].delimiter_ptr = (uint8_t*){delim_ptr}; _targets[{i}].delimiter_len = {delim_size};\n"
                    ));
                }
                if let Some(count_in) = &target.count_in {
                    out.push_str(&format!("{pad}    uint32_t _count_{i} = 0;\n"));
                    out.push_str(&format!(
                        "{pad}    _targets[{i}].count_ptr = &_count_{i};\n"
                    ));
                    let _ = count_in;
                }
            }
            if let Some(pointer) = pointer {
                let pointer_expr =
                    HirExpr::Variable(cobol_hir::HirDataName::simple(pointer.clone()));
                let pointer_value = emit_int_compatible_expr(&pointer_expr, data_items);
                out.push_str(&format!(
                    "{pad}    uint32_t _pointer = (uint32_t)({pointer_value});\n"
                ));
            } else {
                out.push_str(&format!("{pad}    uint32_t _pointer = 1;\n"));
            }
            if let Some(tallying) = tallying {
                let tally_expr =
                    HirExpr::Variable(cobol_hir::HirDataName::simple(tallying.clone()));
                let tally_value = emit_int_compatible_expr(&tally_expr, data_items);
                out.push_str(&format!(
                    "{pad}    uint32_t _tallying = (uint32_t)({tally_value});\n"
                ));
            } else {
                out.push_str(&format!("{pad}    uint32_t _tallying = 0;\n"));
            }
            // Use the first delimiter if specified, otherwise split on spaces
            let (delim_ptr, delim_len) = if let Some(d) = delimiters.first() {
                match &d.value {
                    HirExpr::Literal(HirLiteral::String(s)) => {
                        let escaped = escape_c_string(s);
                        let len = s.len();
                        out.push_str(&format!(
                            "{pad}    static const uint8_t _ustr_delim[] = \"{escaped}\";\n"
                        ));
                        ("(const uint8_t*)_ustr_delim".to_string(), format!("{len}"))
                    }
                    HirExpr::Variable(name) => {
                        let c_d = data_name_to_c_name(name);
                        let d_size = find_data_item_size(&c_d, data_items);
                        let d_ptr = c_ptr_expr(&c_d, data_items);
                        (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                    }
                    HirExpr::Literal(HirLiteral::Zero) => {
                        ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
                    }
                    HirExpr::Literal(HirLiteral::Space) => {
                        ("(const uint8_t*)\" \"".to_string(), "1".to_string())
                    }
                    HirExpr::DataRef(data_ref) => {
                        let c_d = if data_ref.subscripts.is_empty() && data_ref.refmod.is_none() {
                            data_name_to_c_name(&data_ref.name)
                        } else {
                            emit_expr(&d.value)
                        };
                        let d_size = find_data_item_size(&c_d, data_items);
                        let d_ptr = c_ptr_expr(&c_d, data_items);
                        (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                    }
                    _ => ("(const uint8_t*)\" \"".to_string(), "1".to_string()),
                }
            } else {
                ("NULL".to_string(), "0".to_string())
            };
            let (delimiter_sources, delimiter_count) = if delimiters.len() > 1 {
                out.push_str(&format!(
                    "{pad}    CobolStringSource _ustr_delims[{}];\n",
                    delimiters.len()
                ));
                for (i, delimiter) in delimiters.iter().enumerate() {
                    let (ptr, len) = match &delimiter.value {
                        HirExpr::Literal(HirLiteral::String(s)) => {
                            let escaped = escape_c_string(s);
                            out.push_str(&format!(
                                "{pad}    static const uint8_t _ustr_delim_{i}[] = \"{escaped}\";\n"
                            ));
                            (
                                format!("(const uint8_t*)_ustr_delim_{i}"),
                                s.len().to_string(),
                            )
                        }
                        HirExpr::Literal(HirLiteral::Zero) => {
                            ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
                        }
                        HirExpr::Literal(HirLiteral::Space) => {
                            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
                        }
                        HirExpr::Variable(name) => {
                            let c_d = data_name_to_c_name(name);
                            let d_size = find_data_item_size(&c_d, data_items);
                            let d_ptr = c_ptr_expr(&c_d, data_items);
                            (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                        }
                        HirExpr::DataRef(data_ref) => {
                            let c_d = if data_ref.subscripts.is_empty() && data_ref.refmod.is_none()
                            {
                                data_name_to_c_name(&data_ref.name)
                            } else {
                                emit_expr(&delimiter.value)
                            };
                            let d_size = find_data_item_size(&c_d, data_items);
                            let d_ptr = c_ptr_expr(&c_d, data_items);
                            (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                        }
                        _ => ("(const uint8_t*)\" \"".to_string(), "1".to_string()),
                    };
                    out.push_str(&format!(
                        "{pad}    _ustr_delims[{i}].ptr = {ptr}; _ustr_delims[{i}].len = {len}; _ustr_delims[{i}].delim_ptr = NULL; _ustr_delims[{i}].delim_len = 0;\n"
                    ));
                }
                ("_ustr_delims".to_string(), delimiters.len().to_string())
            } else {
                ("NULL".to_string(), "0".to_string())
            };
            let src_ptr = c_ptr_expr(&c_source, data_items);
            let collapse_all = delimiters.first().is_some_and(|d| d.all);
            out.push_str(&format!(
                "{pad}    int32_t _ustr_rc = cobol_unstring((const uint8_t*){src_ptr}, {src_size}, {delim_ptr}, {delim_len}, _targets, {tgt_count}, &_pointer, &_tallying, {}, {delimiter_sources}, {delimiter_count});\n",
                if collapse_all { 1 } else { 0 }
            ));
            if let Some(pointer) = pointer {
                let c_pointer = sanitize_name(pointer);
                emit_store_int(
                    out,
                    &c_pointer,
                    "(int64_t)_pointer",
                    data_items,
                    &format!("{pad}    "),
                );
            }
            if let Some(tallying) = tallying {
                let c_tallying = sanitize_name(tallying);
                emit_store_int(
                    out,
                    &c_tallying,
                    "(int64_t)_tallying",
                    data_items,
                    &format!("{pad}    "),
                );
            }
            for (i, target) in into.iter().enumerate() {
                if let Some(count_in) = &target.count_in {
                    let c_count = sanitize_name(count_in);
                    emit_store_int(
                        out,
                        &c_count,
                        &format!("(int64_t)_count_{i}"),
                        data_items,
                        &format!("{pad}    "),
                    );
                }
            }
            if !on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_ustr_rc != 0) {{\n"));
                for s in on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            if !not_on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_ustr_rc == 0) {{\n"));
                for s in not_on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Accept { target, source, .. } => {
            let c_target = emit_expr(target);
            let target_name = expr_data_name(target);
            if matches!(source, HirAcceptSource::Console) {
                if let Some(item) = target_name
                    .and_then(|name| find_data_item_by_name(name, data_items))
                    .filter(|item| item.screen_info.is_some())
                {
                    out.push_str(&format!("{pad}/* ACCEPT SCREEN {c_target} */\n"));
                    emit_screen_accept(out, item, data_items, &pad);
                    return;
                }
            }
            let target_label = target_name
                .map(|name| name.as_str().to_string())
                .unwrap_or_else(|| c_target.clone());
            let size = target_name
                .and_then(|name| find_data_item_by_name(name, data_items))
                .map(|item| data_item_byte_size(&item.data_type))
                .unwrap_or_else(|| find_data_item_size(&c_target, data_items));
            let comm_binding = ctx.communication_binding(&c_target);
            let implicit_message_count = matches!(source, HirAcceptSource::Console)
                && size == 0
                && comm_binding
                    .as_ref()
                    .and_then(|binding| binding.message_count.as_ref())
                    .is_some();
            out.push_str(&format!("{pad}/* ACCEPT {c_target} */\n"));
            match source {
                HirAcceptSource::Date => {
                    // ACCEPT FROM DATE: YYMMDD (6 digits)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    int32_t _year = 0, _month = 0, _day = 0;\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_runtime_now_parts(&_year, &_month, &_day, NULL, NULL, NULL, NULL, NULL);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    int64_t _dv = (_year % 100) * 10000 + _month * 100 + _day;\n"
                    ));
                    emit_store_int(out, &c_target, "_dv", data_items, &format!("{pad}    "));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::DateYyyymmdd => {
                    // ACCEPT FROM DATE YYYYMMDD: 8 digits
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    int32_t _year = 0, _month = 0, _day = 0;\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_runtime_now_parts(&_year, &_month, &_day, NULL, NULL, NULL, NULL, NULL);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    int64_t _dv = _year * 10000 + _month * 100 + _day;\n"
                    ));
                    emit_store_int(out, &c_target, "_dv", data_items, &format!("{pad}    "));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Day => {
                    // ACCEPT FROM DAY: YYDDD (Julian day)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    int32_t _year = 0, _yday1 = 0;\n"));
                    out.push_str(&format!(
                        "{pad}    cobol_runtime_now_parts(&_year, NULL, NULL, &_yday1, NULL, NULL, NULL, NULL);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    int64_t _dv = (_year % 100) * 1000 + _yday1;\n"
                    ));
                    emit_store_int(out, &c_target, "_dv", data_items, &format!("{pad}    "));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::DayOfWeek => {
                    // ACCEPT FROM DAY-OF-WEEK: 1=Monday ... 7=Sunday
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    int32_t _wday1 = 0;\n"));
                    out.push_str(&format!(
                        "{pad}    cobol_runtime_now_parts(NULL, NULL, NULL, NULL, &_wday1, NULL, NULL, NULL);\n"
                    ));
                    out.push_str(&format!("{pad}    int64_t _dv = _wday1;\n"));
                    emit_store_int(out, &c_target, "_dv", data_items, &format!("{pad}    "));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Time => {
                    // ACCEPT FROM TIME: HHMMSScc (8 digits)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    int32_t _hour = 0, _minute = 0, _sec_centis = 0;\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_runtime_now_parts(NULL, NULL, NULL, NULL, NULL, &_hour, &_minute, &_sec_centis);\n"
                    ));
                    if let Some(item) =
                        target_name.and_then(|name| find_data_item_by_name(name, data_items))
                    {
                        if let HirType::Group { members, .. } = &item.data_type {
                            let numeric_members: Vec<_> = members
                                .iter()
                                .filter_map(|member| match member.data_type {
                                    HirType::Numeric { size, .. } => Some((member, size)),
                                    _ => None,
                                })
                                .collect();
                            if numeric_members.len() >= 3 {
                                let hrs_ref = format!(
                                    "{c_target}__{}",
                                    sanitize_name(&numeric_members[0].0.name)
                                );
                                let mins_ref = format!(
                                    "{c_target}__{}",
                                    sanitize_name(&numeric_members[1].0.name)
                                );
                                let secs_ref = format!(
                                    "{c_target}__{}",
                                    sanitize_name(&numeric_members[2].0.name)
                                );
                                let hrs_ptr = display_numeric_ptr(&hrs_ref);
                                let mins_ptr = display_numeric_ptr(&mins_ref);
                                let secs_ptr = display_numeric_ptr(&secs_ref);
                                out.push_str(&format!(
                                    "{pad}    cobol_store_numeric_display(_hour, {hrs_ptr}, {});\n",
                                    numeric_members[0].1
                                ));
                                out.push_str(&format!(
                                    "{pad}    cobol_store_numeric_display(_minute, {mins_ptr}, {});\n",
                                    numeric_members[1].1
                                ));
                                out.push_str(&format!(
                                    "{pad}    cobol_store_numeric_display(_sec_centis, {secs_ptr}, {});\n",
                                    numeric_members[2].1
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}    int64_t _dv = _hour * 1000000 + _minute * 10000 + _sec_centis;\n"
                                ));
                                emit_store_int(
                                    out,
                                    &c_target,
                                    "_dv",
                                    data_items,
                                    &format!("{pad}    "),
                                );
                            }
                        } else {
                            out.push_str(&format!(
                                "{pad}    int64_t _dv = _hour * 1000000 + _minute * 10000 + _sec_centis;\n"
                            ));
                            emit_store_int(
                                out,
                                &c_target,
                                "_dv",
                                data_items,
                                &format!("{pad}    "),
                            );
                        }
                    } else {
                        out.push_str(&format!(
                            "{pad}    int64_t _dv = _hour * 1000000 + _minute * 10000 + _sec_centis;\n"
                        ));
                        emit_store_int(out, &c_target, "_dv", data_items, &format!("{pad}    "));
                    }
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Environment(env_name) => {
                    let c_env = sanitize_name(env_name);
                    let tgt_ptr = c_ptr_expr(&c_target, data_items);
                    out.push_str(&format!(
                        "{pad}{{ const char* _env = getenv(\"{c_env}\");\n"
                    ));
                    out.push_str(&format!(
                        "{pad}  if (_env) {{ strncpy((char*){tgt_ptr}, _env, {size}); }} }}\n"
                    ));
                }
                HirAcceptSource::Console if !implicit_message_count => {
                    let tgt_ptr = c_ptr_expr(&c_target, data_items);
                    out.push_str(&format!(
                        "{pad}{{ char _accept_buf[{size} + 2]; \
                         if (fgets(_accept_buf, sizeof(_accept_buf), stdin)) {{ \
                         size_t _accept_len = strcspn(_accept_buf, \"\\n\"); \
                         cobol_move_string((const uint8_t*)_accept_buf, _accept_len, \
                         (uint8_t*){tgt_ptr}, {size}); }} }}\n"
                    ));
                }
                HirAcceptSource::MessageCount => {
                    if let Some(binding) = comm_binding.clone() {
                        if let Some(ref message_count) = binding.message_count {
                            let selectors = emit_comm_selectors(&binding, data_items);
                            out.push_str(&format!(
                                "{pad}{{ uint32_t _count = 0; uint32_t _rc = cobol_comm_accept_count((const uint8_t*)\"{c_target}\", {}, &_count, {}, {}, {}, {}, {}, {}, {}, {});\n",
                                c_target.len(),
                                selectors.queue_ptr,
                                selectors.queue_len,
                                selectors.sub1_ptr,
                                selectors.sub1_len,
                                selectors.sub2_ptr,
                                selectors.sub2_len,
                                selectors.sub3_ptr,
                                selectors.sub3_len
                            ));
                            emit_store_int(
                                out,
                                message_count,
                                "(int64_t)_count",
                                data_items,
                                &format!("{pad}    "),
                            );
                            emit_comm_status_updates(
                                out,
                                &c_target,
                                "_rc",
                                None,
                                data_items,
                                &format!("{pad}    "),
                            );
                            emit_debug_communication_event(
                                out,
                                &format!("{pad}    "),
                                &target_label,
                                Some(&binding),
                                data_items,
                                None,
                            );
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            emit_debug_communication_event(
                                out,
                                pad.as_str(),
                                &target_label,
                                Some(&binding),
                                data_items,
                                None,
                            );
                        }
                    }
                }
                HirAcceptSource::Console if implicit_message_count => {
                    if let Some(binding) = comm_binding {
                        if let Some(ref message_count) = binding.message_count {
                            let selectors = emit_comm_selectors(&binding, data_items);
                            out.push_str(&format!(
                                "{pad}{{ uint32_t _count = 0; uint32_t _rc = cobol_comm_accept_count((const uint8_t*)\"{c_target}\", {}, &_count, {}, {}, {}, {}, {}, {}, {}, {});\n",
                                c_target.len(),
                                selectors.queue_ptr,
                                selectors.queue_len,
                                selectors.sub1_ptr,
                                selectors.sub1_len,
                                selectors.sub2_ptr,
                                selectors.sub2_len,
                                selectors.sub3_ptr,
                                selectors.sub3_len
                            ));
                            emit_store_int(
                                out,
                                message_count,
                                "(int64_t)_count",
                                data_items,
                                &format!("{pad}    "),
                            );
                            emit_comm_status_updates(
                                out,
                                &c_target,
                                "_rc",
                                None,
                                data_items,
                                &format!("{pad}    "),
                            );
                            emit_debug_communication_event(
                                out,
                                &format!("{pad}    "),
                                &target_label,
                                Some(&binding),
                                data_items,
                                None,
                            );
                            out.push_str(&format!("{pad}}}\n"));
                        }
                    }
                }
                HirAcceptSource::Console => {
                    if size == 0 {
                        emit_debug_communication_event(
                            out,
                            pad.as_str(),
                            &target_label,
                            comm_binding.as_ref(),
                            data_items,
                            None,
                        );
                    }
                }
            }
        }
        HirStatement::Enable {
            mode,
            terminal,
            target,
            key,
            ..
        } => {
            let c_target = sanitize_name(target);
            let (c_key_ptr, c_key_len) = emit_comm_arg(key, data_items);
            let binding = ctx.communication_binding(&c_target);
            let selectors = binding
                .as_ref()
                .map(|binding| emit_comm_selectors(binding, data_items))
                .unwrap_or_default();
            let source = binding
                .as_ref()
                .map(|binding| {
                    emit_optional_comm_item(binding.symbolic_source.as_deref(), data_items)
                })
                .unwrap_or_else(null_comm_arg);
            let (dest_layout, dest_count_expr, error_key_layout) = binding
                .as_ref()
                .map(|binding| {
                    let mut dest_layout =
                        emit_optional_comm_area_layout(binding.destination.as_deref(), data_items);
                    if let Some(legacy_table_count) = binding.destination_table_count {
                        if legacy_table_count != 0 {
                            dest_layout.count = legacy_table_count.to_string();
                        }
                    }
                    (
                        dest_layout,
                        binding
                            .destination_count
                            .as_ref()
                            .map(|name| emit_numeric_expr_for_var(name, data_items))
                            .unwrap_or_else(|| "0".to_string()),
                        emit_optional_comm_area_layout(binding.error_key.as_deref(), data_items),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        emit_optional_comm_area_layout(None, data_items),
                        "0".to_string(),
                        emit_optional_comm_area_layout(None, data_items),
                    )
                });
            let dest_ptr = format!("(const uint8_t*){}", dest_layout.ptr);
            let error_key_ptr = format!("(uint8_t*){}", error_key_layout.ptr);
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_enable((const uint8_t*)\"{c_target}\", {}, {}, {}, {c_key_ptr}, {c_key_len}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
                c_target.len(),
                emit_comm_mode(mode),
                if *terminal { 1 } else { 0 },
                selectors.queue_ptr,
                selectors.queue_len,
                selectors.sub1_ptr,
                selectors.sub1_len,
                selectors.sub2_ptr,
                selectors.sub2_len,
                selectors.sub3_ptr,
                selectors.sub3_len,
                source.0,
                source.1,
                dest_ptr,
                dest_layout.item_len,
                dest_layout.stride,
                dest_count_expr,
                dest_layout.count,
                dest_layout.area_len,
                error_key_ptr,
                error_key_layout.item_len,
                error_key_layout.stride,
                error_key_layout.count,
                error_key_layout.area_len
            ));
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                None,
                data_items,
                &format!("{pad}    "),
            );
            emit_debug_communication_event(
                out,
                &format!("{pad}    "),
                target.as_str(),
                binding.as_ref(),
                data_items,
                None,
            );
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Disable {
            mode,
            terminal,
            target,
            key,
            ..
        } => {
            let c_target = sanitize_name(target);
            let (c_key_ptr, c_key_len) = emit_comm_arg(key, data_items);
            let binding = ctx.communication_binding(&c_target);
            let selectors = binding
                .as_ref()
                .map(|binding| emit_comm_selectors(binding, data_items))
                .unwrap_or_default();
            let source = binding
                .as_ref()
                .map(|binding| {
                    emit_optional_comm_item(binding.symbolic_source.as_deref(), data_items)
                })
                .unwrap_or_else(null_comm_arg);
            let (dest_layout, dest_count_expr, error_key_layout) = binding
                .as_ref()
                .map(|binding| {
                    let mut dest_layout =
                        emit_optional_comm_area_layout(binding.destination.as_deref(), data_items);
                    if let Some(legacy_table_count) = binding.destination_table_count {
                        if legacy_table_count != 0 {
                            dest_layout.count = legacy_table_count.to_string();
                        }
                    }
                    (
                        dest_layout,
                        binding
                            .destination_count
                            .as_ref()
                            .map(|name| emit_numeric_expr_for_var(name, data_items))
                            .unwrap_or_else(|| "0".to_string()),
                        emit_optional_comm_area_layout(binding.error_key.as_deref(), data_items),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        emit_optional_comm_area_layout(None, data_items),
                        "0".to_string(),
                        emit_optional_comm_area_layout(None, data_items),
                    )
                });
            let dest_ptr = format!("(const uint8_t*){}", dest_layout.ptr);
            let error_key_ptr = format!("(uint8_t*){}", error_key_layout.ptr);
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_disable((const uint8_t*)\"{c_target}\", {}, {}, {}, {c_key_ptr}, {c_key_len}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
                c_target.len(),
                emit_comm_mode(mode),
                if *terminal { 1 } else { 0 },
                selectors.queue_ptr,
                selectors.queue_len,
                selectors.sub1_ptr,
                selectors.sub1_len,
                selectors.sub2_ptr,
                selectors.sub2_len,
                selectors.sub3_ptr,
                selectors.sub3_len,
                source.0,
                source.1,
                dest_ptr,
                dest_layout.item_len,
                dest_layout.stride,
                dest_count_expr,
                dest_layout.count,
                dest_layout.area_len,
                error_key_ptr,
                error_key_layout.item_len,
                error_key_layout.stride,
                error_key_layout.count,
                error_key_layout.area_len
            ));
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                None,
                data_items,
                &format!("{pad}    "),
            );
            emit_debug_communication_event(
                out,
                &format!("{pad}    "),
                target.as_str(),
                binding.as_ref(),
                data_items,
                None,
            );
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Send {
            target,
            from,
            with,
            replacing_line,
            ..
        } => {
            let c_target = sanitize_name(target);
            let (c_from_ptr, c_from_len) = if let Some(from) = from {
                emit_comm_arg(from, data_items)
            } else {
                ("NULL".to_string(), "0".to_string())
            };
            let binding = ctx.communication_binding(&c_target);
            let effective_len = binding
                .as_ref()
                .and_then(|binding| binding.text_length.as_ref())
                .map(|name| emit_numeric_expr_for_var(name, data_items))
                .unwrap_or_else(|| c_from_len.clone());
            let (dest_layout, dest_count_expr, error_key_layout) = binding
                .as_ref()
                .map(|binding| {
                    let mut dest_layout =
                        emit_optional_comm_area_layout(binding.destination.as_deref(), data_items);
                    if let Some(legacy_table_count) = binding.destination_table_count {
                        if legacy_table_count != 0 {
                            dest_layout.count = legacy_table_count.to_string();
                        }
                    }
                    (
                        dest_layout,
                        binding
                            .destination_count
                            .as_ref()
                            .map(|name| emit_numeric_expr_for_var(name, data_items))
                            .unwrap_or_else(|| "0".to_string()),
                        emit_optional_comm_area_layout(binding.error_key.as_deref(), data_items),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        emit_optional_comm_area_layout(None, data_items),
                        "0".to_string(),
                        emit_optional_comm_area_layout(None, data_items),
                    )
                });
            let (option_kind, option_value) = match with {
                Some(cobol_hir::HirSendOption::Emi) => ("1".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Egi) => ("2".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Esi) => ("3".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Identifier(expr)) => {
                    ("4".to_string(), emit_expr_as_numeric(expr))
                }
                None => ("0".to_string(), "0".to_string()),
            };
            let dest_ptr = format!("(const uint8_t*){}", dest_layout.ptr);
            let error_key_ptr = format!("(uint8_t*){}", error_key_layout.ptr);
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_send((const uint8_t*)\"{c_target}\", {}, {c_from_ptr}, {c_from_len}, {effective_len}, {option_kind}, {option_value}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
                c_target.len(),
                if *replacing_line { 1 } else { 0 },
                dest_ptr,
                dest_layout.item_len,
                dest_layout.stride,
                dest_count_expr,
                dest_layout.count,
                dest_layout.area_len,
                error_key_ptr,
                error_key_layout.item_len,
                error_key_layout.stride,
                error_key_layout.count,
                error_key_layout.area_len
            ));
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                None,
                data_items,
                &format!("{pad}    "),
            );
            emit_debug_communication_event(
                out,
                &format!("{pad}    "),
                target.as_str(),
                binding.as_ref(),
                data_items,
                None,
            );
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Receive {
            target,
            mode,
            into,
            no_data,
            ..
        } => {
            let c_target = sanitize_name(target);
            let c_into = sanitize_name(into);
            let into_ptr = c_ptr_expr(&c_into, data_items);
            let into_len = find_data_item_size(&c_into, data_items);
            let binding = ctx.communication_binding(&c_target);
            let selectors = binding
                .as_ref()
                .map(|binding| emit_comm_selectors(binding, data_items))
                .unwrap_or_default();
            out.push_str(&format!(
                "{pad}{{ uint32_t _text_len = 0; uint32_t _rc = cobol_comm_receive((const uint8_t*)\"{c_target}\", {}, {}, (uint8_t*){into_ptr}, {into_len}, &_text_len, {}, {}, {}, {}, {}, {}, {}, {});\n",
                c_target.len(),
                match mode {
                    HirReceiveMode::Message => 1,
                    HirReceiveMode::Segment => 2,
                },
                selectors.queue_ptr,
                selectors.queue_len,
                selectors.sub1_ptr,
                selectors.sub1_len,
                selectors.sub2_ptr,
                selectors.sub2_len,
                selectors.sub3_ptr,
                selectors.sub3_len
            ));
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                Some("_text_len"),
                data_items,
                &format!("{pad}    "),
            );
            let receive_debug_condition = if no_data.is_empty() {
                None
            } else {
                Some("_rc != 10")
            };
            emit_debug_communication_event(
                out,
                &format!("{pad}    "),
                target.as_str(),
                binding.as_ref(),
                data_items,
                receive_debug_condition,
            );
            if let Some(binding) = env.ctx.communication_binding(&c_target) {
                if let Some(end_key) = binding.end_key {
                    out.push_str(&format!(
                        "{pad}    cobol_move_string((const uint8_t*)((cobol_comm_last_end_key((const uint8_t*)\"{c_target}\", {}) == 0) ? \"0\" : ((cobol_comm_last_end_key((const uint8_t*)\"{c_target}\", {}) == 1) ? \"1\" : ((cobol_comm_last_end_key((const uint8_t*)\"{c_target}\", {}) == 2) ? \"2\" : \"3\"))), 1, (uint8_t*){}, {});\n",
                        c_target.len(),
                        c_target.len(),
                        c_target.len(),
                        c_ptr_expr(&end_key, data_items),
                        find_data_item_size(&end_key, data_items)
                    ));
                }
            }
            if !no_data.is_empty() {
                out.push_str(&format!("{pad}    if (_rc == 10) {{\n"));
                for stmt in no_data {
                    emit_statement(
                        out,
                        stmt,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Purge { target, .. } => {
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_purge((const uint8_t*)\"{c_target}\", {});\n",
                c_target.len()
            ));
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                None,
                data_items,
                &format!("{pad}    "),
            );
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Sort {
            file_name,
            keys,
            using,
            giving,
            input_procedure,
            output_procedure,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let rec_len = find_record_len(&record_var, data_items);
            out.push_str(&format!("{pad}/* SORT {c_name} */\n"));
            // Flatten all key fields across HirSortKey entries
            let mut flat_keys: Vec<(&str, bool)> = Vec::new();
            for key in keys {
                let ascending = matches!(key.order, cobol_hir::HirSortOrder::Ascending);
                for field in &key.fields {
                    flat_keys.push((field.as_str(), ascending));
                }
            }
            let key_count = if flat_keys.is_empty() {
                1
            } else {
                flat_keys.len()
            };
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!("{pad}    SortKey _sort_keys[{key_count}];\n"));
            if flat_keys.is_empty() {
                out.push_str(&format!(
                    "{pad}    _sort_keys[0].offset = 0; _sort_keys[0].length = {rec_len}; _sort_keys[0].ascending = 1; _sort_keys[0].key_type = 0;\n"
                ));
            } else {
                let needs_conv = sort_record_needs_conversion(&record_var, data_items);
                for (i, (field_name, ascending)) in flat_keys.iter().enumerate() {
                    let asc_val: u8 = if *ascending { 1 } else { 0 };
                    let mut kt = sort_key_type_for_field(field_name, data_items);
                    // Override key type for CobolDecimal fields when the sort
                    // buffer stores their value as int64_t binary.
                    let field_is_decimal = {
                        let fc = sanitize_name(field_name);
                        find_original_data_item_by_sanitized_name(&fc, data_items)
                            .is_some_and(|item| needs_decimal(&item.data_type))
                    };
                    let field_is_display_numeric = {
                        let fc = sanitize_name(field_name);
                        display_numeric_c_expr_info(&fc, data_items).is_some()
                    };
                    let mut key_len_override: Option<u32> = None;
                    if needs_conv && field_is_decimal && !field_is_display_numeric {
                        kt = 1; // SORT_KEY_SIGNED_BINARY
                        key_len_override = Some(8); // sizeof(int64_t)
                    }
                    // Use file-format offsets/sizes for sort keys (matches flat buffer layout)
                    if let Some((offset, size)) =
                        find_sort_field_offset_and_size(field_name, &record_var, data_items)
                    {
                        let sz = key_len_override.unwrap_or(size);
                        out.push_str(&format!(
                            "{pad}    _sort_keys[{i}].offset = {offset}; _sort_keys[{i}].length = {sz}; _sort_keys[{i}].ascending = {asc_val}; _sort_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                        ));
                    } else if let Some((offset, size)) =
                        find_field_offset_and_size(field_name, &record_var, data_items)
                    {
                        let sz = key_len_override.unwrap_or(size);
                        out.push_str(&format!(
                            "{pad}    _sort_keys[{i}].offset = {offset}; _sort_keys[{i}].length = {sz}; _sort_keys[{i}].ascending = {asc_val}; _sort_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                        ));
                    } else {
                        let field_c = sanitize_name(field_name);
                        let field_size = key_len_override
                            .unwrap_or_else(|| find_data_item_size(&field_c, data_items));
                        out.push_str(&format!(
                            "{pad}    _sort_keys[{i}].offset = 0; _sort_keys[{i}].length = {field_size}; _sort_keys[{i}].ascending = {asc_val}; _sort_keys[{i}].key_type = {kt}; /* {field_name} (no offset) */\n"
                        ));
                    }
                }
            }
            if !using.is_empty() {
                // Read records from USING files into a dynamic buffer, then sort
                out.push_str(&format!("{pad}    uint32_t _sort_capacity = 64;\n"));
                out.push_str(&format!("{pad}    uint32_t _sort_count = 0;\n"));
                out.push_str(&format!(
                    "{pad}    uint8_t* _sort_buf = (uint8_t*)malloc(_sort_capacity * {rec_len});\n"
                ));
                for u in using {
                    let c_using = sanitize_name(u);
                    let using_record = resolve_file_record(&c_using);
                    let using_org = sort_file_runtime_org(ctx, &c_using, &using_record, data_items);
                    let using_path = ctx.file_assignment(&c_using).unwrap_or(&c_using);
                    let using_path_escaped = escape_c_string(using_path);
                    let using_path_len = using_path.len();
                    out.push_str(&format!(
                        "{pad}    /* USING {c_using}: read all records */\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_file_open(FILE_ID_{c_using}, (const uint8_t*)\"{using_path_escaped}\", {using_path_len}, {using_org}, 0, 0, {rec_len});\n"
                    ));
                    if ctx.file_is_variable_record(&c_using) {
                        out.push_str(&format!(
                            "{pad}    cobol_file_set_variable(FILE_ID_{c_using});\n"
                        ));
                    }
                    out.push_str(&format!("{pad}    while (1) {{\n"));
                    out.push_str(&format!(
                        "{pad}        int32_t _rc = cobol_file_read_next(FILE_ID_{c_using}, (uint8_t*)&_sort_buf[_sort_count * {rec_len}], {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        if (_rc != 0) break;\n"));
                    out.push_str(&format!("{pad}        _sort_count++;\n"));
                    out.push_str(&format!(
                        "{pad}        if (_sort_count >= _sort_capacity) {{\n"
                    ));
                    out.push_str(&format!("{pad}            _sort_capacity *= 2;\n"));
                    out.push_str(&format!(
                        "{pad}            _sort_buf = (uint8_t*)realloc(_sort_buf, _sort_capacity * {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        }}\n"));
                    out.push_str(&format!("{pad}    }}\n"));
                    out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_using});\n"));
                }
                // If there's an input procedure too (USING + INPUT PROCEDURE)
                if let Some((proc_name, thru)) = input_procedure {
                    let c_proc = sanitize_name(proc_name);
                    let event_name = escape_c_string(proc_name);
                    out.push_str(&format!("{pad}    /* INPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!(
                        "{pad}    _sort_buf_id = cobol_sort_buffer_init({rec_len});\n"
                    ));
                    if should_emit_debug_events() {
                        out.push_str(&format!(
                            "{pad}    _set_debug_event(\"{event_name}\", \"SORT INPUT\", \"\");\n"
                        ));
                    }
                    emit_sort_procedure_call(
                        out,
                        proc_name,
                        thru.as_deref(),
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        should_emit_debug_events(),
                    );
                    out.push_str(&format!("{pad}    _goto_target = 0;\n"));
                }
                // Convert CobolDecimal fields from display to binary in sort buffer
                if sort_record_needs_conversion(&record_var, data_items) {
                    out.push_str(&format!(
                        "{pad}    /* Convert CobolDecimal fields to binary for sorting */\n"
                    ));
                    emit_sort_buf_display_to_binary(
                        out,
                        &record_var,
                        data_items,
                        rec_len,
                        "_sort_buf",
                        "_sort_count",
                        &format!("{pad}    "),
                    );
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort(_sort_buf, _sort_count, {rec_len}, _sort_keys, {key_count});\n"
                ));
                if !giving.is_empty() {
                    for g in giving {
                        let c_giving = sanitize_name(g);
                        let giving_record = resolve_file_record(&c_giving);
                        let giving_org =
                            sort_file_runtime_org(ctx, &c_giving, &giving_record, data_items);
                        let giving_path = ctx.file_assignment(&c_giving).unwrap_or(&c_giving);
                        let giving_path_escaped = escape_c_string(giving_path);
                        let giving_path_len = giving_path.len();
                        out.push_str(&format!(
                            "{pad}    /* GIVING {c_giving}: write sorted records */\n"
                        ));
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_giving}, (const uint8_t*)\"{giving_path_escaped}\", {giving_path_len}, {giving_org}, 0, 1, {rec_len});\n"
                        ));
                        if ctx.file_is_variable_record(&c_giving) {
                            out.push_str(&format!(
                                "{pad}    cobol_file_set_variable(FILE_ID_{c_giving});\n"
                            ));
                        }
                        out.push_str(&format!(
                            "{pad}    for (uint32_t _si = 0; _si < _sort_count; _si++) {{\n"
                        ));
                        out.push_str(&format!(
                            "{pad}        cobol_file_write(FILE_ID_{c_giving}, (const uint8_t*)&_sort_buf[_si * {rec_len}], {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}    }}\n"));
                        out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_giving});\n"));
                    }
                }
                if let Some((proc_name, thru)) = output_procedure {
                    let c_proc = sanitize_name(proc_name);
                    let event_name = escape_c_string(proc_name);
                    out.push_str(&format!("{pad}    /* OUTPUT PROCEDURE {c_proc} */\n"));
                    // Copy sorted display-format records into sort buffer for RETURN
                    out.push_str(&format!(
                        "{pad}    _sort_buf_id = cobol_sort_buffer_init({rec_len});\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    for (uint32_t _si = 0; _si < _sort_count; _si++) {{\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        cobol_sort_buffer_release(_sort_buf_id, &_sort_buf[_si * {rec_len}], {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}    }}\n"));
                    if should_emit_debug_events() {
                        out.push_str(&format!(
                            "{pad}    _set_debug_event(\"{event_name}\", \"SORT OUTPUT\", \"\");\n"
                        ));
                    }
                    emit_sort_procedure_call(
                        out,
                        proc_name,
                        thru.as_deref(),
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        should_emit_debug_events(),
                    );
                    out.push_str(&format!("{pad}    cobol_sort_buffer_free(_sort_buf_id);\n"));
                }
                out.push_str(&format!("{pad}    free(_sort_buf);\n"));
            } else if input_procedure.is_some() || output_procedure.is_some() {
                // INPUT/OUTPUT PROCEDURE with runtime sort buffer
                out.push_str(&format!(
                    "{pad}    _sort_buf_id = cobol_sort_buffer_init({rec_len});\n"
                ));
                if let Some((proc_name, thru)) = input_procedure {
                    let c_proc = sanitize_name(proc_name);
                    let event_name = escape_c_string(proc_name);
                    out.push_str(&format!("{pad}    /* INPUT PROCEDURE {c_proc} */\n"));
                    if should_emit_debug_events() {
                        out.push_str(&format!(
                            "{pad}    _set_debug_event(\"{event_name}\", \"SORT INPUT\", \"\");\n"
                        ));
                    }
                    emit_sort_procedure_call(
                        out,
                        proc_name,
                        thru.as_deref(),
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        should_emit_debug_events(),
                    );
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort_buffer_sort(_sort_buf_id, _sort_keys, {key_count});\n"
                ));
                if !giving.is_empty() {
                    out.push_str(&format!("{pad}    uint8_t _sort_giving_rec[{rec_len}];\n"));
                    for g in giving {
                        let c_giving = sanitize_name(g);
                        let giving_record = resolve_file_record(&c_giving);
                        let giving_org =
                            sort_file_runtime_org(ctx, &c_giving, &giving_record, data_items);
                        let giving_path = ctx.file_assignment(&c_giving).unwrap_or(&c_giving);
                        let giving_path_escaped = escape_c_string(giving_path);
                        let giving_path_len = giving_path.len();
                        out.push_str(&format!(
                            "{pad}    /* GIVING {c_giving}: write sorted records */\n"
                        ));
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_giving}, (const uint8_t*)\"{giving_path_escaped}\", {giving_path_len}, {giving_org}, 0, 1, {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}    while (1) {{\n"));
                        out.push_str(&format!(
                            "{pad}        uint32_t _gfs = cobol_sort_buffer_return(_sort_buf_id, _sort_giving_rec, {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}        if (_gfs == 10) break;\n"));
                        out.push_str(&format!(
                            "{pad}        cobol_file_write(FILE_ID_{c_giving}, _sort_giving_rec, {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}    }}\n"));
                        out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_giving});\n"));
                    }
                }
                if let Some((proc_name, thru)) = output_procedure {
                    let c_proc = sanitize_name(proc_name);
                    let event_name = escape_c_string(proc_name);
                    out.push_str(&format!("{pad}    /* OUTPUT PROCEDURE {c_proc} */\n"));
                    if should_emit_debug_events() {
                        out.push_str(&format!(
                            "{pad}    _set_debug_event(\"{event_name}\", \"SORT OUTPUT\", \"\");\n"
                        ));
                    }
                    emit_sort_procedure_call(
                        out,
                        proc_name,
                        thru.as_deref(),
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                        should_emit_debug_events(),
                    );
                }
                out.push_str(&format!("{pad}    cobol_sort_buffer_free(_sort_buf_id);\n"));
            } else {
                // No USING: sort in-place
                out.push_str(&format!(
                    "{pad}    cobol_sort((uint8_t*)&{record_var}, 0, {rec_len}, _sort_keys, {key_count});\n"
                ));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Inspect { target, kind, .. } => {
            let (c_target, target_size, target_size_expr) = match target {
                HirExpr::DataRef(data_ref)
                    if data_ref.subscripts.is_empty() && data_ref.refmod.is_none() =>
                {
                    let c_name = data_name_to_c_name(&data_ref.name);
                    let size = find_data_item_size(&c_name, data_items);
                    let size_expr = alphanumeric_expr_len_expr(target, data_items)
                        .unwrap_or_else(|| size.to_string());
                    (c_name, size, size_expr)
                }
                _ => {
                    let (ptr, len) = emit_alphanumeric_operand(target, data_items);
                    let size = len.parse::<u32>().unwrap_or(0);
                    let temp_name = format!("_inspect_target_{}", out.len());
                    out.push_str(&format!("{pad}uint8_t* {temp_name} = (uint8_t*){ptr};\n"));
                    (temp_name, size, len)
                }
            };
            out.push_str(&format!("{pad}/* INSPECT {c_target} */\n"));
            match kind {
                cobol_hir::HirInspectKind::Tallying { tallying } => {
                    if tallying.len() <= 1 {
                        emit_inspect_tallying(
                            out,
                            &c_target,
                            &target_size_expr,
                            tallying,
                            data_items,
                            &pad,
                        );
                    } else {
                        emit_inspect_tallying_series(
                            out,
                            &c_target,
                            &target_size_expr,
                            tallying,
                            data_items,
                            &pad,
                        );
                    }
                }
                cobol_hir::HirInspectKind::Replacing { replacing } => {
                    emit_inspect_replacing(
                        out,
                        &c_target,
                        target_size,
                        replacing,
                        data_items,
                        &pad,
                    );
                }
                cobol_hir::HirInspectKind::TallyingReplacing {
                    tallying,
                    replacing,
                } => {
                    emit_inspect_tallying_series(
                        out,
                        &c_target,
                        &target_size_expr,
                        tallying,
                        data_items,
                        &pad,
                    );
                    emit_inspect_replacing_series(
                        out,
                        &c_target,
                        target_size,
                        replacing,
                        data_items,
                        &pad,
                    );
                }
                cobol_hir::HirInspectKind::Converting {
                    from,
                    to,
                    before_after,
                } => {
                    let c_from = emit_inspect_operand(out, from, "conv_from", data_items, &pad);
                    let c_to = emit_inspect_operand(out, to, "conv_to", data_items, &pad);
                    let insp_tgt_ptr = c_ptr_expr(&c_target, data_items);
                    if before_after.is_empty() {
                        out.push_str(&format!(
                            "{pad}cobol_inspect_converting((uint8_t*){insp_tgt_ptr}, {target_size}, {}, {}, {}, {});\n",
                            c_from.0, c_from.1, c_to.0, c_to.1
                        ));
                    } else {
                        out.push_str(&format!("{pad}{{\n"));
                        out.push_str(&format!(
                            "{pad}    uint8_t* _insp_base = (uint8_t*){insp_tgt_ptr};\n"
                        ));
                        out.push_str(&format!("{pad}    uint32_t _insp_start = 0;\n"));
                        out.push_str(&format!("{pad}    uint32_t _insp_end = {target_size};\n"));
                        for (j, ba) in before_after
                            .iter()
                            .enumerate()
                            .filter(|(_, ba)| !ba.is_before)
                            .chain(
                                before_after
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, ba)| ba.is_before),
                            )
                        {
                            let marker_label = format!("conv_ba{j}");
                            let (marker_ptr, marker_len) = emit_inspect_operand(
                                out,
                                &ba.value,
                                &marker_label,
                                data_items,
                                &pad,
                            );
                            out.push_str(&format!(
                                "{pad}    const uint8_t* _insp_marker_{j} = {marker_ptr};\n"
                            ));
                            out.push_str(&format!(
                                "{pad}    uint32_t _insp_marker_len_{j} = {marker_len};\n"
                            ));
                            if ba.is_before {
                                out.push_str(&format!(
                                    "{pad}    if (_insp_marker_len_{j} == 0 || _insp_marker_len_{j} > _insp_end - _insp_start) {{\n"
                                ));
                                out.push_str(&format!("{pad}    }} else {{\n"));
                                out.push_str(&format!(
                                    "{pad}        uint32_t _insp_found = _insp_end;\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}        for (uint32_t _i = _insp_start; _i + _insp_marker_len_{j} <= _insp_end; _i++) {{\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}            if (memcmp(_insp_base + _i, _insp_marker_{j}, _insp_marker_len_{j}) == 0) {{ _insp_found = _i; break; }}\n"
                                ));
                                out.push_str(&format!("{pad}        }}\n"));
                                out.push_str(&format!(
                                    "{pad}        if (_insp_found != _insp_end) _insp_end = _insp_found;\n"
                                ));
                                out.push_str(&format!("{pad}    }}\n"));
                            } else {
                                out.push_str(&format!(
                                    "{pad}    if (_insp_marker_len_{j} == 0 || _insp_marker_len_{j} > _insp_end - _insp_start) {{\n"
                                ));
                                out.push_str(&format!("{pad}        _insp_start = _insp_end;\n"));
                                out.push_str(&format!("{pad}    }} else {{\n"));
                                out.push_str(&format!(
                                    "{pad}        uint32_t _insp_found = _insp_end;\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}        for (uint32_t _i = _insp_start; _i + _insp_marker_len_{j} <= _insp_end; _i++) {{\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}            if (memcmp(_insp_base + _i, _insp_marker_{j}, _insp_marker_len_{j}) == 0) {{ _insp_found = _i; break; }}\n"
                                ));
                                out.push_str(&format!("{pad}        }}\n"));
                                out.push_str(&format!(
                                    "{pad}        _insp_start = (_insp_found == _insp_end) ? _insp_end : _insp_found + _insp_marker_len_{j};\n"
                                ));
                                out.push_str(&format!("{pad}    }}\n"));
                            }
                        }
                        out.push_str(&format!(
                            "{pad}    cobol_inspect_converting(_insp_base + _insp_start, _insp_end - _insp_start, {}, {}, {}, {});\n",
                            c_from.0, c_from.1, c_to.0, c_to.1
                        ));
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            }
        }
        HirStatement::StopRun { .. } => {
            out.push_str(&format!("{pad}cobol_stop_run();\n"));
        }
        HirStatement::Goback { .. } => {
            out.push_str(&format!("{pad}cobol_goback();\n"));
        }
        HirStatement::ExitProgram { .. } => {
            if ctx.is_subprogram() {
                out.push_str(&format!("{pad}cobol_goback(); /* EXIT PROGRAM */\n"));
            } else {
                out.push_str(&format!("{pad}cobol_stop_run(); /* EXIT PROGRAM */\n"));
            }
        }
        HirStatement::ExitParagraph { .. } => {
            out.push_str(&format!("{pad}return; /* EXIT PARAGRAPH */\n"));
        }
        HirStatement::Continue { .. } => {
            out.push_str(&format!("{pad}/* CONTINUE */\n"));
        }
        HirStatement::Label { target } => {
            let c_name = transfer_target_c_name(target, paragraphs);
            let target_name = escape_c_string(target.name());
            let label = format!("lbl_{c_name}");
            let is_new = with_active_context(|ctx| ctx.mark_label_emitted(label.clone()));
            if is_new {
                emit_fallthrough_debug_event(out, &pad, &target_name, "FALL THROUGH");
                out.push_str(&format!("{label}:;\n"));
            }
        }
        // --- COBOL 2002+ statements ---
        HirStatement::Invoke {
            object,
            method,
            params,
            returning,
            ..
        } => {
            let c_obj = emit_expr(object);
            let args: Vec<_> = params.iter().map(emit_expr).collect();
            let args_str = args.join(", ");
            if let Some(ret) = returning {
                let c_ret = sanitize_name(ret);
                out.push_str(&format!(
                    "{pad}{c_ret} = cobol_invoke((void*){c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
                    params.len()
                ));
            } else {
                out.push_str(&format!(
                    "{pad}cobol_invoke((void*){c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
                    params.len()
                ));
            }
        }
        HirStatement::Raise { exception, .. } => {
            out.push_str(&format!("{pad}cobol_raise(\"{exception}\");\n"));
        }
        HirStatement::Resume { target, .. } => {
            if let Some(t) = target {
                let c_target = sanitize_name(t);
                out.push_str(&format!("{pad}cobol_resume(\"{c_target}\");\n"));
            } else {
                out.push_str(&format!("{pad}cobol_resume(NULL);\n"));
            }
        }
        HirStatement::Allocate {
            target,
            returning,
            char_count,
            ..
        } => {
            let c_target = sanitize_name(target);
            let size_expr = if let Some(count_expr) = char_count {
                emit_int_compatible_expr(count_expr, data_items)
            } else {
                format!("sizeof({c_target})")
            };
            if let Some(ret) = returning {
                let c_ret = sanitize_name(ret);
                out.push_str(&format!(
                    "{pad}{c_ret} = malloc({size_expr}); /* ALLOCATE */\n"
                ));
            } else {
                out.push_str(&format!(
                    "{pad}{c_target} = malloc({size_expr}); /* ALLOCATE */\n"
                ));
            }
        }
        HirStatement::Free { targets, .. } => {
            for target in targets {
                let c_target = sanitize_name(target);
                out.push_str(&format!("{pad}free({c_target}); {c_target} = NULL;\n"));
            }
        }
        // --- COBOL 2014+ statements ---
        HirStatement::Validate { target, .. } => {
            emit_validate_statement(out, target, data_items, &pad);
        }
        HirStatement::JsonGenerate { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_json_generate(&{c_source}, sizeof({c_source}), (uint8_t*){c_target}, sizeof({c_target})); /* JSON GENERATE */\n"
            ));
        }
        HirStatement::JsonParse { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_json_parse((const uint8_t*){c_source}, strlen({c_source}), &{c_target}, sizeof({c_target})); /* JSON PARSE */\n"
            ));
        }
        HirStatement::XmlGenerate { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_xml_generate(&{c_source}, sizeof({c_source}), \"{c_source}\", {}, (uint8_t*){c_target}, sizeof({c_target})); /* XML GENERATE */\n",
                c_source.len()
            ));
        }
        HirStatement::XmlParse {
            source,
            processing_procedure,
            ..
        } => {
            let c_source = sanitize_name(source);
            let c_proc = sanitize_name(processing_procedure);
            out.push_str(&format!(
                "{pad}/* XML PARSE {c_source} PROCESSING PROCEDURE {c_proc} */\n"
            ));
            out.push_str(&format!(
                "{pad}cobol_xml_parse((const uint8_t*){c_source}, strlen((const char*){c_source}), _xml_cb_{c_proc});\n"
            ));
        }
        // --- File I/O: additional statements ---
        HirStatement::Start {
            file_name,
            key,
            op,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let mode_val = match op {
                HirStartRelation::Equal => 0,
                HirStartRelation::GreaterThan => 1,
                HirStartRelation::GreaterEqual | HirStartRelation::NotLessThan => 2,
            };
            out.push_str(&format!("{pad}/* START {c_name} */\n"));
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            out.push_str(&format!("{pad}{{\n"));
            let start_call = if let Some(key_name) = key {
                let record_var = resolve_file_record(&c_name);
                let resolved_key = resolve_record_key_item(key_name, &record_var, data_items);
                let c_key = resolved_key
                    .as_ref()
                    .map(|(c_key, _, _)| c_key.clone())
                    .unwrap_or_else(|| sanitize_name(key_name));
                if ctx.file_organization(&c_name) == Some(2) {
                    let rel_expr = emit_numeric_expr_for_var(&c_key, data_items);
                    format!(
                        "cobol_file_start_relative(FILE_ID_{c_name}, (uint64_t)({rel_expr}), {mode_val})"
                    )
                } else {
                    let key_size = resolved_key
                        .as_ref()
                        .map(|(_, size, _)| *size)
                        .unwrap_or_else(|| find_data_item_size(&c_key, data_items));
                    let is_key_group = resolved_key
                        .as_ref()
                        .map(|(_, _, is_group)| *is_group)
                        .unwrap_or_else(|| {
                            find_data_item(key_name.as_str(), data_items)
                                .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }))
                        });
                    let addr_prefix = if is_key_group { "&" } else { "" };
                    let key_offset = if ctx.file_organization(&c_name) == Some(3) {
                        find_field_offset_and_size(key_name, &record_var, data_items)
                            .map(|(offset, _)| offset)
                            .unwrap_or(u32::MAX)
                    } else {
                        0
                    };
                    format!(
                        "cobol_file_start(FILE_ID_{c_name}, (const uint8_t*){addr_prefix}{c_key}, {key_size}, {key_offset}, {mode_val})"
                    )
                }
            } else {
                format!("cobol_file_start(FILE_ID_{c_name}, NULL, 0, 0, {mode_val})")
            };
            if needs_rc {
                out.push_str(&format!("{pad}    uint32_t _src = {start_call};\n"));
                if fs_map.contains_key(&c_name) {
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_src",
                        fs_map,
                        false,
                        &format!("FILE_MODE_{c_name}"),
                        &format!("{pad}    "),
                    );
                }
                if has_declaratives && invalid_key.is_empty() {
                    let dispatch_fn =
                        with_active_context(|ctx| ctx.file_declarative_dispatch_fn().to_string());
                    out.push_str(&format!("{pad}    if (_src != 0) {{\n"));
                    out.push_str(&format!(
                        "{pad}        {dispatch_fn}(\"{c_name}\", FILE_MODE_{c_name}, _src);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}        if (_goto_target) goto _goto_dispatch;\n"
                    ));
                    out.push_str(&format!("{pad}    }}\n"));
                }
                emit_debug_spaces_event(out, &format!("{pad}    "), file_name.as_str());
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_src != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_src == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
            } else {
                out.push_str(&format!("{pad}    {start_call};\n"));
                emit_debug_spaces_event(out, &format!("{pad}    "), file_name.as_str());
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Return {
            file_name,
            into,
            at_end,
            not_at_end,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let rec_len = find_record_len(&record_var, data_items);
            let needs_conv = sort_record_needs_conversion(&record_var, data_items);
            let into_target = into.as_ref().map(|(into_var, into_subs)| {
                let into_len = find_data_item_size(&sanitize_name(into_var), data_items);
                let copy_len = if into_len == 0 {
                    rec_len
                } else {
                    rec_len.min(into_len)
                };
                let target = if into_subs.is_empty() {
                    sanitize_name(into_var)
                } else {
                    emit_subscript_access(&HirDataName::simple(into_var.clone()), into_subs)
                };
                (target, copy_len)
            });
            out.push_str(&format!("{pad}/* RETURN {c_name} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            if needs_conv {
                // Sort buffer contains display-format bytes; deserialize into SD record
                out.push_str(&format!("{pad}    uint8_t _sort_flat[{rec_len}];\n"));
                out.push_str(&format!(
                    "{pad}    uint32_t _fs = cobol_sort_buffer_return(_sort_buf_id, _sort_flat, {rec_len});\n"
                ));
            } else if into_target.is_some() {
                // No conversion needed but INTO specified: read into SD record, then copy
                out.push_str(&format!(
                    "{pad}    uint32_t _fs = cobol_sort_buffer_return(_sort_buf_id, (uint8_t*)&{record_var}, {rec_len});\n"
                ));
            } else {
                out.push_str(&format!(
                    "{pad}    uint32_t _fs = cobol_sort_buffer_return(_sort_buf_id, (uint8_t*)&{record_var}, {rec_len});\n"
                ));
            }
            if !at_end.is_empty() || !not_at_end.is_empty() {
                out.push_str(&format!("{pad}    if (_fs == 10) {{\n"));
                out.push_str(&format!("{pad}        /* AT END */\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                if !not_at_end.is_empty() {
                    out.push_str(&format!("{pad}    }} else {{\n"));
                    out.push_str(&format!("{pad}        /* NOT AT END */\n"));
                    for s in not_at_end {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            // After AT END handling: deserialize and copy if needed
            if needs_conv {
                out.push_str(&format!("{pad}    if (_fs != 10) {{\n"));
                // Deserialize display bytes into SD record struct
                emit_sort_record_deserialize(
                    out,
                    &record_var,
                    data_items,
                    "_sort_flat",
                    &format!("{pad}        "),
                );
                // If INTO, also copy display bytes to the INTO target
                if let Some((ref into_t, copy_len)) = into_target {
                    out.push_str(&format!(
                        "{pad}        memcpy(&{into_t}, _sort_flat, {copy_len});\n"
                    ));
                }
                out.push_str(&format!("{pad}    }}\n"));
            } else if let Some((ref into_t, copy_len)) = into_target {
                // No conversion but INTO: copy SD record to INTO target
                out.push_str(&format!(
                    "{pad}    if (_fs != 10) memcpy(&{into_t}, &{record_var}, {copy_len});\n"
                ));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Cancel { programs, .. } => {
            for prog in programs {
                let prog_name = match prog {
                    HirExpr::Literal(HirLiteral::String(s)) => sanitize_name(s),
                    HirExpr::Variable(name) => sanitize_name(name),
                    _ => emit_expr(prog),
                };
                out.push_str(&format!(
                    "{pad}/* CANCEL {prog_name} -- releases loaded program resources */\n"
                ));
            }
        }
        HirStatement::Merge {
            file_name,
            keys,
            using,
            giving,
            output_procedure,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let rec_len = find_record_len(&record_var, data_items);
            out.push_str(&format!("{pad}/* MERGE {c_name} */\n"));
            if !using.is_empty() {
                let using_names: Vec<_> = using.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* USING {} */\n", using_names.join(", ")));
            }
            if !giving.is_empty() {
                let giving_names: Vec<_> = giving.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* GIVING {} */\n", giving_names.join(", ")));
            }
            let mut flat_keys: Vec<(&str, bool)> = Vec::new();
            for key in keys {
                let ascending = matches!(key.order, cobol_hir::HirSortOrder::Ascending);
                for field in &key.fields {
                    flat_keys.push((field.as_str(), ascending));
                }
            }
            let key_count = if flat_keys.is_empty() {
                1
            } else {
                flat_keys.len()
            };
            if let Some((proc_name, thru)) = output_procedure.as_ref().filter(|_| giving.is_empty())
            {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    SortKey _merge_keys[{key_count}];\n"));
                if flat_keys.is_empty() {
                    out.push_str(&format!(
                        "{pad}    _merge_keys[0].offset = 0; _merge_keys[0].length = {rec_len}; _merge_keys[0].ascending = 1; _merge_keys[0].key_type = 0;\n"
                    ));
                } else {
                    let needs_conv = sort_record_needs_conversion(&record_var, data_items);
                    for (i, (field_name, ascending)) in flat_keys.iter().enumerate() {
                        let asc_val: u8 = if *ascending { 1 } else { 0 };
                        let mut kt = sort_key_type_for_field(field_name, data_items);
                        let field_is_decimal = {
                            let fc = sanitize_name(field_name);
                            find_original_data_item_by_sanitized_name(&fc, data_items)
                                .is_some_and(|item| needs_decimal(&item.data_type))
                        };
                        let field_is_display_numeric = {
                            let fc = sanitize_name(field_name);
                            display_numeric_c_expr_info(&fc, data_items).is_some()
                        };
                        let mut key_len_override: Option<u32> = None;
                        if needs_conv && field_is_decimal && !field_is_display_numeric {
                            kt = 1;
                            key_len_override = Some(8);
                        }
                        if let Some((offset, size)) =
                            find_sort_field_offset_and_size(field_name, &record_var, data_items)
                        {
                            let sz = key_len_override.unwrap_or(size);
                            out.push_str(&format!(
                                "{pad}    _merge_keys[{i}].offset = {offset}; _merge_keys[{i}].length = {sz}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                            ));
                        } else if let Some((offset, size)) =
                            find_field_offset_and_size(field_name, &record_var, data_items)
                        {
                            let sz = key_len_override.unwrap_or(size);
                            out.push_str(&format!(
                                "{pad}    _merge_keys[{i}].offset = {offset}; _merge_keys[{i}].length = {sz}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                            ));
                        } else {
                            let field_c = sanitize_name(field_name);
                            let field_size = key_len_override
                                .unwrap_or_else(|| find_data_item_size(&field_c, data_items));
                            out.push_str(&format!(
                                "{pad}    _merge_keys[{i}].offset = 0; _merge_keys[{i}].length = {field_size}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} (no offset) */\n"
                            ));
                        }
                    }
                }
                out.push_str(&format!("{pad}    uint32_t _merge_capacity = 64;\n"));
                out.push_str(&format!("{pad}    uint32_t _merge_count = 0;\n"));
                out.push_str(&format!(
                    "{pad}    uint8_t* _merge_buf = (uint8_t*)malloc(_merge_capacity * {rec_len});\n"
                ));
                for input_file in using {
                    let c_input = sanitize_name(input_file);
                    let input_record = resolve_file_record(&c_input);
                    let input_org = sort_file_runtime_org(ctx, &c_input, &input_record, data_items);
                    let input_path = ctx.file_assignment(&c_input).unwrap_or(&c_input);
                    let input_path_escaped = escape_c_string(input_path);
                    let input_path_len = input_path.len();
                    out.push_str(&format!("{pad}    /* MERGE USING {c_input} */\n"));
                    out.push_str(&format!(
                        "{pad}    cobol_file_open(FILE_ID_{c_input}, (const uint8_t*)\"{input_path_escaped}\", {input_path_len}, {input_org}, 0, 0, {rec_len});\n"
                    ));
                    if ctx.file_is_variable_record(&c_input) {
                        out.push_str(&format!(
                            "{pad}    cobol_file_set_variable(FILE_ID_{c_input});\n"
                        ));
                    }
                    out.push_str(&format!("{pad}    while (1) {{\n"));
                    out.push_str(&format!(
                        "{pad}        int32_t _rc = cobol_file_read_next(FILE_ID_{c_input}, (uint8_t*)&_merge_buf[_merge_count * {rec_len}], {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        if (_rc != 0) break;\n"));
                    out.push_str(&format!("{pad}        _merge_count++;\n"));
                    out.push_str(&format!(
                        "{pad}        if (_merge_count >= _merge_capacity) {{\n"
                    ));
                    out.push_str(&format!("{pad}            _merge_capacity *= 2;\n"));
                    out.push_str(&format!(
                        "{pad}            _merge_buf = (uint8_t*)realloc(_merge_buf, _merge_capacity * {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        }}\n"));
                    out.push_str(&format!("{pad}    }}\n"));
                    out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_input});\n"));
                }
                if sort_record_needs_conversion(&record_var, data_items) {
                    out.push_str(&format!(
                        "{pad}    /* Convert CobolDecimal fields to binary for merging */\n"
                    ));
                    emit_sort_buf_display_to_binary(
                        out,
                        &record_var,
                        data_items,
                        rec_len,
                        "_merge_buf",
                        "_merge_count",
                        &format!("{pad}    "),
                    );
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort(_merge_buf, _merge_count, {rec_len}, _merge_keys, {key_count});\n"
                ));
                out.push_str(&format!(
                    "{pad}    _sort_buf_id = cobol_sort_buffer_init({rec_len});\n"
                ));
                out.push_str(&format!(
                    "{pad}    for (uint32_t _mi = 0; _mi < _merge_count; _mi++) {{\n"
                ));
                out.push_str(&format!(
                    "{pad}        cobol_sort_buffer_release(_sort_buf_id, &_merge_buf[_mi * {rec_len}], {rec_len});\n"
                ));
                out.push_str(&format!("{pad}    }}\n"));
                let proc_debug_name = escape_c_string(proc_name);
                if should_emit_debug_events() {
                    out.push_str(&format!(
                        "{pad}    _set_debug_event(\"{proc_debug_name}\", \"MERGE OUTPUT\", \"\");\n"
                    ));
                }
                emit_sort_procedure_call(
                    out,
                    proc_name,
                    thru.as_deref(),
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                    should_emit_debug_events(),
                );
                out.push_str(&format!("{pad}    cobol_sort_buffer_free(_sort_buf_id);\n"));
                out.push_str(&format!("{pad}    free(_merge_buf);\n"));
                out.push_str(&format!("{pad}}}\n"));
            } else {
                let input_count = using.len();
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!(
                    "{pad}    uint32_t _merge_inputs[{input_count}];\n"
                ));
                for (i, input_file) in using.iter().enumerate() {
                    let c_input = sanitize_name(input_file);
                    let input_record = resolve_file_record(&c_input);
                    let input_org = sort_file_runtime_org(ctx, &c_input, &input_record, data_items);
                    let input_path = ctx.file_assignment(&c_input).unwrap_or(&c_input);
                    let input_path_escaped = escape_c_string(input_path);
                    let input_path_len = input_path.len();
                    out.push_str(&format!("{pad}    /* MERGE USING {c_input} */\n"));
                    out.push_str(&format!(
                    "{pad}    cobol_file_open(FILE_ID_{c_input}, (const uint8_t*)\"{input_path_escaped}\", {input_path_len}, {input_org}, 0, 0, {rec_len});\n"
                ));
                    if ctx.file_is_variable_record(&c_input) {
                        out.push_str(&format!(
                            "{pad}    cobol_file_set_variable(FILE_ID_{c_input});\n"
                        ));
                    }
                    out.push_str(&format!(
                        "{pad}    _merge_inputs[{i}] = FILE_ID_{c_input};\n"
                    ));
                }
                out.push_str(&format!("{pad}    SortKey _merge_keys[{key_count}];\n"));
                if flat_keys.is_empty() {
                    out.push_str(&format!(
                    "{pad}    _merge_keys[0].offset = 0; _merge_keys[0].length = {rec_len}; _merge_keys[0].ascending = 1; _merge_keys[0].key_type = 0;\n"
                ));
                } else {
                    let needs_conv = sort_record_needs_conversion(&record_var, data_items);
                    for (i, (field_name, ascending)) in flat_keys.iter().enumerate() {
                        let asc_val: u8 = if *ascending { 1 } else { 0 };
                        let mut kt = sort_key_type_for_field(field_name, data_items);
                        let field_is_decimal = {
                            let fc = sanitize_name(field_name);
                            find_original_data_item_by_sanitized_name(&fc, data_items)
                                .is_some_and(|item| needs_decimal(&item.data_type))
                        };
                        let field_is_display_numeric = {
                            let fc = sanitize_name(field_name);
                            display_numeric_c_expr_info(&fc, data_items).is_some()
                        };
                        let mut key_len_override: Option<u32> = None;
                        if needs_conv && field_is_decimal && !field_is_display_numeric {
                            kt = 1;
                            key_len_override = Some(8);
                        }
                        if let Some((offset, size)) =
                            find_sort_field_offset_and_size(field_name, &record_var, data_items)
                        {
                            let sz = key_len_override.unwrap_or(size);
                            out.push_str(&format!(
                            "{pad}    _merge_keys[{i}].offset = {offset}; _merge_keys[{i}].length = {sz}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                        ));
                        } else if let Some((offset, size)) =
                            find_field_offset_and_size(field_name, &record_var, data_items)
                        {
                            let sz = key_len_override.unwrap_or(size);
                            out.push_str(&format!(
                            "{pad}    _merge_keys[{i}].offset = {offset}; _merge_keys[{i}].length = {sz}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                        ));
                        } else {
                            let field_c = sanitize_name(field_name);
                            let field_size = key_len_override
                                .unwrap_or_else(|| find_data_item_size(&field_c, data_items));
                            out.push_str(&format!(
                            "{pad}    _merge_keys[{i}].offset = 0; _merge_keys[{i}].length = {field_size}; _merge_keys[{i}].ascending = {asc_val}; _merge_keys[{i}].key_type = {kt}; /* {field_name} (no offset) */\n"
                        ));
                        }
                    }
                }
                let output_file_id = if let Some(first_giving) = giving.first() {
                    let c_giving = sanitize_name(first_giving);
                    let giving_record = resolve_file_record(&c_giving);
                    let giving_org =
                        sort_file_runtime_org(ctx, &c_giving, &giving_record, data_items);
                    let giving_path = ctx.file_assignment(&c_giving).unwrap_or(&c_giving);
                    let giving_path_escaped = escape_c_string(giving_path);
                    let giving_path_len = giving_path.len();
                    out.push_str(&format!("{pad}    /* MERGE GIVING {c_giving} */\n"));
                    out.push_str(&format!(
                    "{pad}    cobol_file_open(FILE_ID_{c_giving}, (const uint8_t*)\"{giving_path_escaped}\", {giving_path_len}, {giving_org}, 0, 1, {rec_len});\n"
                ));
                    if ctx.file_is_variable_record(&c_giving) {
                        out.push_str(&format!(
                            "{pad}    cobol_file_set_variable(FILE_ID_{c_giving});\n"
                        ));
                    }
                    format!("FILE_ID_{c_giving}")
                } else {
                    format!("FILE_ID_{c_name}")
                };
                out.push_str(&format!(
                "{pad}    cobol_merge(_merge_inputs, {input_count}, {output_file_id}, _merge_keys, {key_count}, {rec_len});\n"
            ));
                for input_file in using {
                    let c_input = sanitize_name(input_file);
                    out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_input});\n"));
                }
                if let Some(first_giving) = giving.first() {
                    let c_giving = sanitize_name(first_giving);
                    out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_giving});\n"));
                }
                if let Some(first_giving) = giving.first() {
                    let c_first = sanitize_name(first_giving);
                    let first_record = resolve_file_record(&c_first);
                    let first_org = sort_file_runtime_org(ctx, &c_first, &first_record, data_items);
                    let first_path = ctx.file_assignment(&c_first).unwrap_or(&c_first);
                    let first_path_escaped = escape_c_string(first_path);
                    let first_path_len = first_path.len();
                    for extra_giving in giving.iter().skip(1) {
                        let c_extra = sanitize_name(extra_giving);
                        let extra_record = resolve_file_record(&c_extra);
                        let extra_org =
                            sort_file_runtime_org(ctx, &c_extra, &extra_record, data_items);
                        let extra_path = ctx.file_assignment(&c_extra).unwrap_or(&c_extra);
                        let extra_path_escaped = escape_c_string(extra_path);
                        let extra_path_len = extra_path.len();
                        out.push_str(&format!(
                            "{pad}    /* MERGE GIVING {c_extra}: duplicate first giving */\n"
                        ));
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_first}, (const uint8_t*)\"{first_path_escaped}\", {first_path_len}, {first_org}, 0, 0, {rec_len});\n"
                        ));
                        if ctx.file_is_variable_record(&c_first) {
                            out.push_str(&format!(
                                "{pad}    cobol_file_set_variable(FILE_ID_{c_first});\n"
                            ));
                        }
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_extra}, (const uint8_t*)\"{extra_path_escaped}\", {extra_path_len}, {extra_org}, 0, 1, {rec_len});\n"
                        ));
                        if ctx.file_is_variable_record(&c_extra) {
                            out.push_str(&format!(
                                "{pad}    cobol_file_set_variable(FILE_ID_{c_extra});\n"
                            ));
                        }
                        out.push_str(&format!("{pad}    while (1) {{\n"));
                        out.push_str(&format!(
                            "{pad}        uint8_t _merge_copy_rec[{rec_len}];\n"
                        ));
                        out.push_str(&format!(
                            "{pad}        int32_t _copy_rc = cobol_file_read_next(FILE_ID_{c_first}, _merge_copy_rec, {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}        if (_copy_rc != 0) break;\n"));
                        out.push_str(&format!(
                            "{pad}        cobol_file_write(FILE_ID_{c_extra}, _merge_copy_rec, {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}    }}\n"));
                        out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_first});\n"));
                        out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_extra});\n"));
                    }
                }
                if let Some((proc_name, thru)) = output_procedure {
                    let proc_debug_name = escape_c_string(proc_name);
                    if should_emit_debug_events() {
                        out.push_str(&format!(
                        "{pad}    _set_debug_event(\"{proc_debug_name}\", \"MERGE OUTPUT\", \"\");\n"
                    ));
                    }
                    let c_proc = sanitize_name(proc_name);
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        if c_thru != c_proc {
                            out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                        }
                    }
                }
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Release {
            record_name, from, ..
        } => {
            let c_name = sanitize_name(record_name);
            let rec_len = find_record_len(&c_name, data_items);
            let needs_conv = sort_record_needs_conversion(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            let source_len = from
                .as_ref()
                .and_then(|from_expr| alphanumeric_expr_len_c_expr(from_expr, data_items))
                .unwrap_or_else(|| rec_len.to_string());
            out.push_str(&format!("{pad}/* RELEASE {c_name} */\n"));
            if needs_conv {
                // Serialize struct to display format before releasing
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    uint8_t _sort_flat[{rec_len}];\n"));
                // If FROM is specified, first move to the record
                if from.is_some() {
                    out.push_str(&format!("{pad}    memset(&{c_name}, ' ', {rec_len});\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _release_len = (uint32_t)({source_len});\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    if (_release_len > {rec_len}) _release_len = {rec_len};\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    memcpy(&{c_name}, &{source}, _release_len);\n"
                    ));
                }
                emit_sort_record_serialize(
                    out,
                    &c_name,
                    data_items,
                    "_sort_flat",
                    &format!("{pad}    "),
                );
                out.push_str(&format!(
                    "{pad}    cobol_sort_buffer_release(_sort_buf_id, _sort_flat, {rec_len});\n"
                ));
                out.push_str(&format!("{pad}}}\n"));
            } else if from.is_some() {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    uint8_t _release_flat[{rec_len}];\n"));
                out.push_str(&format!(
                    "{pad}    memset(_release_flat, ' ', {rec_len});\n"
                ));
                out.push_str(&format!(
                    "{pad}    uint32_t _release_len = (uint32_t)({source_len});\n"
                ));
                out.push_str(&format!(
                    "{pad}    if (_release_len > {rec_len}) _release_len = {rec_len};\n"
                ));
                out.push_str(&format!(
                    "{pad}    memcpy(_release_flat, &{source}, _release_len);\n"
                ));
                out.push_str(&format!(
                    "{pad}    cobol_sort_buffer_release(_sort_buf_id, _release_flat, {rec_len});\n"
                ));
                out.push_str(&format!("{pad}}}\n"));
            } else {
                out.push_str(&format!(
                    "{pad}cobol_sort_buffer_release(_sort_buf_id, (const uint8_t*)&{source}, {rec_len});\n"
                ));
            }
        }
        // --- Table handling: SEARCH ---
        HirStatement::Search {
            table_name,
            all,
            varying,
            at_end,
            when_clauses,
            ..
        } => {
            let c_table = sanitize_name(table_name);
            let table_index = find_first_index_name(&c_table, data_items);
            let varying_index = varying
                .as_ref()
                .filter(|v| {
                    !*all && index_belongs_to_table(&c_table, &sanitize_name(v), data_items)
                })
                .map(sanitize_name);
            let c_idx = varying_index
                .or_else(|| table_index.clone())
                .or_else(|| varying.as_ref().map(sanitize_name))
                .unwrap_or_else(|| format!("{c_table}_IDX"));
            let varying_c = varying.as_ref().map(sanitize_name);
            let sync_varying = varying_c.as_ref().filter(|v| **v != c_idx);
            let max_occurs = find_occurs_bound_expr(&c_table, data_items);
            let inner_pad = "    ".repeat(indent + 1);
            let inner2_pad = "    ".repeat(indent + 2);
            out.push_str(&format!("{pad}/* SEARCH {c_table} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!("{inner_pad}int _search_found = 0;\n"));
            if *all {
                out.push_str(&format!("{inner_pad}{c_idx} = 1;\n"));
            }
            out.push_str(&format!(
                "{inner_pad}while ({c_idx} <= ({max_occurs})) {{\n"
            ));
            if let Some(c_varying) = sync_varying {
                emit_search_varying_sync(out, c_varying, &c_idx, data_items, &inner2_pad);
            }
            for when in when_clauses {
                let cond = emit_condition(&when.condition, data_items);
                out.push_str(&format!("{inner2_pad}if ({cond}) {{\n"));
                let body_pad_level = indent + 3;
                for s in &when.body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        body_pad_level,
                    );
                }
                out.push_str(&format!("{inner2_pad}    _search_found = 1; break;\n"));
                out.push_str(&format!("{inner2_pad}}}\n"));
            }
            out.push_str(&format!("{inner2_pad}{c_idx}++;\n"));
            if let Some(c_varying) = sync_varying {
                emit_search_varying_sync(out, c_varying, &c_idx, data_items, &inner2_pad);
            }
            out.push_str(&format!("{inner_pad}}}\n"));
            if !at_end.is_empty() {
                out.push_str(&format!("{inner_pad}if (!_search_found) {{\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{inner_pad}}}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        // --- Report writer statements ---
        HirStatement::Initiate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* INITIATE {c_name} */\n"));
            }
            emit_store_int(out, "LINE_COUNTER", "0", data_items, &pad);
            emit_store_int(out, "PAGE_COUNTER", "1", data_items, &pad);
        }
        HirStatement::Generate { report_name, .. } => {
            let c_name = sanitize_name(report_name);
            out.push_str(&format!("{pad}/* GENERATE {c_name} */\n"));
            if !emit_report_generate_line(out, report_name, data_items, &pad) {
                out.push_str(&format!("{pad}printf(\"{c_name}\\n\");\n"));
                out.push_str(&format!("{pad}fflush(stdout);\n"));
            }
            let first_detail_line = report_initial_line_counter(data_items) + 1;
            let last_detail_line = report_last_detail_line(data_items);
            let line_expr = format!(
                "(LINE_COUNTER == 0 ? {first_detail_line} : \
                 (LINE_COUNTER >= {last_detail_line} ? {first_detail_line} : LINE_COUNTER + 1))"
            );
            out.push_str(&format!(
                "{pad}if (LINE_COUNTER >= {last_detail_line}) {{ PAGE_COUNTER += 1; }}\n"
            ));
            emit_store_int(out, "LINE_COUNTER", &line_expr, data_items, &pad);
        }
        HirStatement::Terminate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* TERMINATE {c_name} */\n"));
            }
        }
    }
}

fn emit_report_generate_line(
    out: &mut String,
    report_name: &str,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let Some(item) = find_data_item(report_name, data_items) else {
        return false;
    };
    let HirType::Group { members, .. } = &item.data_type else {
        return false;
    };

    let mut fields = Vec::new();
    for member in members {
        collect_report_fields(&mut fields, member, None, None);
    }
    if fields.is_empty() {
        return false;
    }

    fields.sort_by_key(|si| (si.line.unwrap_or(1), si.column.unwrap_or(1)));

    let mut current_line = 1_u32;
    let mut current_col = 1_u32;
    for si in fields {
        let line = si.line.unwrap_or(1);
        let column = si.column.unwrap_or(current_col);
        while current_line < line {
            out.push_str(&format!("{pad}cobol_display_newline();\n"));
            current_line += 1;
            current_col = 1;
        }
        if column > current_col {
            emit_report_spaces(out, column - current_col, pad);
            current_col = column;
        }
        let width = emit_report_field(out, &si, data_items, pad);
        current_col = current_col.saturating_add(width.max(1));
    }
    out.push_str(&format!("{pad}cobol_display_newline();\n"));
    true
}

fn collect_report_fields(
    fields: &mut Vec<cobol_hir::HirScreenInfo>,
    item: &HirDataItem,
    inherited_line: Option<u32>,
    inherited_column: Option<u32>,
) {
    let merged = item
        .screen_info
        .as_ref()
        .map(|si| cobol_hir::HirScreenInfo {
            line: si.line.or(inherited_line),
            column: si.column.or(inherited_column),
            blank_screen: si.blank_screen,
            blank_line: si.blank_line,
            highlight: si.highlight,
            reverse_video: si.reverse_video,
            source: si.source.clone(),
            using_field: si.using_field.clone(),
            value: si.value.clone(),
            picture: si.picture.clone(),
        });

    let next_line = merged.as_ref().and_then(|si| si.line).or(inherited_line);
    let next_column = merged
        .as_ref()
        .and_then(|si| si.column)
        .or(inherited_column);

    if let Some(si) = merged {
        if si.value.is_some() || si.source.is_some() || si.using_field.is_some() {
            fields.push(si);
        }
    }

    if let HirType::Group { members, .. } = &item.data_type {
        for member in members {
            collect_report_fields(fields, member, next_line, next_column);
        }
    }
}

fn emit_report_spaces(out: &mut String, count: u32, pad: &str) {
    if count == 0 {
        return;
    }
    out.push_str(&format!(
        "{pad}for (uint32_t _rw_i = 0; _rw_i < {count}; _rw_i++) {{ cobol_display_space(); }}\n"
    ));
}

fn emit_report_field(
    out: &mut String,
    si: &cobol_hir::HirScreenInfo,
    data_items: &[HirDataItem],
    pad: &str,
) -> u32 {
    if let Some(value) = si.value.as_ref() {
        let escaped = escape_c_string(value);
        let len = value.len() as u32;
        out.push_str(&format!(
            "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
        ));
        return len;
    }

    let Some(source) = si.source.as_ref().or(si.using_field.as_ref()) else {
        return 0;
    };
    let c_name = sanitize_name(source);
    let item = find_data_item(source, data_items);
    let width = item
        .map(|item| data_item_byte_size(&item.data_type))
        .unwrap_or_else(|| find_data_item_size(&c_name, data_items));
    let is_alpha = item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
    if is_alpha {
        out.push_str(&format!(
            "{pad}cobol_display_string((const uint8_t*){c_name}, {width});\n"
        ));
    } else if display_numeric_c_expr_metadata(&c_name, data_items).is_some() {
        emit_display_numeric_storage(out, &c_name, item, data_items, pad);
    } else if let Some(disp_size) = grp_display_size(&c_name, data_items) {
        let c_name_ptr = display_numeric_const_ptr(&c_name);
        out.push_str(&format!(
            "{pad}cobol_display_int(cobol_display_to_int64({c_name_ptr}, {disp_size}));\n"
        ));
    } else {
        out.push_str(&format!("{pad}cobol_display_int({c_name});\n"));
    }
    width
}

fn emit_search_varying_sync(
    out: &mut String,
    c_varying: &str,
    c_idx: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(disp_size) = grp_display_size(c_varying, data_items) {
        let c_target_ptr = display_numeric_ptr(c_varying);
        out.push_str(&format!(
            "{pad}cobol_store_numeric_display({c_idx}, {c_target_ptr}, {disp_size});\n"
        ));
    } else {
        out.push_str(&format!("{pad}{c_varying} = {c_idx};\n"));
    }
}

fn report_initial_line_counter(data_items: &[HirDataItem]) -> i64 {
    data_items
        .iter()
        .find_map(|item| {
            item.name
                .strip_prefix("RW-DUMMY-MARKER-FD-")
                .and_then(|value| value.split("-LD-").next())
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(0)
}

fn report_last_detail_line(data_items: &[HirDataItem]) -> i64 {
    data_items
        .iter()
        .find_map(|item| {
            item.name
                .split("-LD-")
                .nth(1)
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(9999)
}

fn has_linage_counter(data_items: &[HirDataItem]) -> bool {
    data_items
        .iter()
        .any(|item| sanitize_name(&item.name) == "LINAGE_COUNTER")
}

#[allow(clippy::too_many_arguments)]
fn emit_successful_write_followups(
    out: &mut String,
    advancing: Option<&HirWriteAdvancing>,
    at_eop: &[HirStatement],
    not_at_eop: &[HirStatement],
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
    condition: Option<&str>,
) {
    let needs_linage = has_linage_counter(data_items);
    let needs_eop = !at_eop.is_empty() || !not_at_eop.is_empty();
    if !needs_linage && !needs_eop {
        return;
    }

    let pad = "    ".repeat(indent);
    if let Some(condition) = condition {
        out.push_str(&format!("{pad}if ({condition}) {{\n"));
    } else {
        out.push_str(&format!("{pad}{{\n"));
    }

    let inner_indent = indent + 1;
    let inner_pad = "    ".repeat(inner_indent);
    if needs_eop {
        out.push_str(&format!("{inner_pad}int _linage_eop = 0;\n"));
    }

    if needs_linage {
        emit_linage_counter_update(
            out,
            advancing,
            data_items,
            &inner_pad,
            needs_eop.then_some("_linage_eop"),
        );
    }

    if needs_eop {
        out.push_str(&format!("{inner_pad}if (_linage_eop) {{\n"));
        for s in at_eop {
            emit_statement(
                out,
                s,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                inner_indent + 1,
            );
        }
        if !not_at_eop.is_empty() {
            out.push_str(&format!("{inner_pad}}} else {{\n"));
            for s in not_at_eop {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    inner_indent + 1,
                );
            }
        }
        out.push_str(&format!("{inner_pad}}}\n"));
    }

    out.push_str(&format!("{pad}}}\n"));
}

fn emit_linage_counter_update(
    out: &mut String,
    advancing: Option<&HirWriteAdvancing>,
    data_items: &[HirDataItem],
    prefix: &str,
    eop_var: Option<&str>,
) {
    match advancing {
        Some(HirWriteAdvancing::Page) => {
            if let Some(eop_var) = eop_var {
                out.push_str(&format!("{prefix}{eop_var} = 1;\n"));
            }
            out.push_str(&format!("{prefix}LINAGE_COUNTER = 1;\n"));
        }
        Some(HirWriteAdvancing::Lines(expr)) => {
            let value = emit_int_compatible_expr(expr, data_items);
            emit_linage_counter_add(out, prefix, &format!("({value})"), data_items, eop_var);
        }
        None => {
            emit_linage_counter_add(out, prefix, "1", data_items, eop_var);
        }
    }
}

fn emit_linage_counter_add(
    out: &mut String,
    prefix: &str,
    amount: &str,
    data_items: &[HirDataItem],
    eop_var: Option<&str>,
) {
    let page_lines = linage_page_lines_expr(data_items);
    if let Some(eop_var) = eop_var {
        out.push_str(&format!("{prefix}{{\n"));
        out.push_str(&format!(
            "{prefix}    int64_t _linage_next = LINAGE_COUNTER + ({amount});\n"
        ));
        out.push_str(&format!(
            "{prefix}    {eop_var} = _linage_next > {page_lines};\n"
        ));
        out.push_str(&format!(
            "{prefix}    LINAGE_COUNTER = {eop_var} ? 1 : _linage_next;\n"
        ));
        out.push_str(&format!("{prefix}}}\n"));
    } else {
        out.push_str(&format!(
            "{prefix}LINAGE_COUNTER = (LINAGE_COUNTER + ({amount}) > {page_lines} ? 1 : LINAGE_COUNTER + ({amount}));\n"
        ));
    }
}

fn linage_page_lines_expr(data_items: &[HirDataItem]) -> String {
    for item in data_items {
        if let Some(value) = item.name.strip_prefix("LINAGE-MARKER-LINES-NAME-") {
            return emit_numeric_expr_for_var(&sanitize_name(value), data_items);
        }
        if let Some(value) = item.name.strip_prefix("LINAGE-MARKER-LINES-") {
            if value.parse::<i64>().is_ok() {
                return value.to_string();
            }
        }
    }
    i64::MAX.to_string()
}

fn find_occurs_bound_expr(c_name: &str, data_items: &[HirDataItem]) -> String {
    find_occurs_bound_expr_in(c_name, data_items)
        .unwrap_or_else(|| find_occurs_count(c_name, data_items).to_string())
}

fn index_belongs_to_table(c_table: &str, c_index: &str, data_items: &[HirDataItem]) -> bool {
    for item in data_items {
        if sanitize_name(&item.name) == c_table {
            return item
                .indexed_by
                .iter()
                .any(|index_name| sanitize_name(index_name) == c_index);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if index_belongs_to_table(c_table, c_index, members) {
                return true;
            }
        }
    }
    false
}

fn find_occurs_bound_expr_in(c_name: &str, data_items: &[HirDataItem]) -> Option<String> {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            let max_occurs = item.occurs.unwrap_or(10);
            return Some(if let Some(depending) = &item.occurs_depending_on {
                let value = emit_int_compatible_expr(
                    &HirExpr::Variable(depending.clone().into()),
                    data_items,
                );
                format!("({value} < 0 ? 0 : ({value} > {max_occurs} ? {max_occurs} : {value}))")
            } else {
                max_occurs.to_string()
            });
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_occurs_bound_expr_in(c_name, members) {
                return Some(found);
            }
        }
    }
    None
}

fn variable_record_io_len_expr(ctx: &CodegenContext, c_file: &str, record_len: u32) -> String {
    ctx.variable_record_depending(c_file)
        .map(|depending| {
            format!(
                "((uint32_t)({depending} > {record_len} ? {record_len} : ({depending} < 0 ? 0 : {depending})))"
            )
        })
        .unwrap_or_else(|| record_len.to_string())
}

fn variable_record_boundary_error_expr(ctx: &CodegenContext, c_file: &str) -> Option<String> {
    let depending = ctx.variable_record_depending(c_file)?;
    let (min_len, max_len) = ctx.variable_record_bounds(c_file)?;
    Some(format!(
        "({depending} < {min_len} || {depending} > {max_len})"
    ))
}

fn alphanumeric_expr_len_c_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> Option<String> {
    match expr {
        HirExpr::DataRef(data_ref) => {
            let item = find_data_item(&data_ref.name, data_items)?;
            if let Some(refmod) = &data_ref.refmod {
                let c_name = data_name_to_c_name(&data_ref.name);
                let full_size = find_data_item_size(&c_name, data_items);
                return Some(if let Some(length) = &refmod.length {
                    if let HirExpr::Literal(HirLiteral::Integer(n)) = length.as_ref() {
                        (*n).max(0).to_string()
                    } else {
                        full_size.to_string()
                    }
                } else if let HirExpr::Literal(HirLiteral::Integer(start)) = refmod.start.as_ref() {
                    full_size
                        .saturating_sub((*start).saturating_sub(1) as u32)
                        .to_string()
                } else {
                    full_size.to_string()
                });
            }
            effective_item_len_c_expr(item, data_items).or_else(|| {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
                )
                .then(|| {
                    let c_name = data_name_to_c_name(&data_ref.name);
                    find_data_item_size(&c_name, data_items).to_string()
                })
            })
        }
        HirExpr::Variable(name) => {
            let item = find_data_item(name, data_items)?;
            effective_item_len_c_expr(item, data_items).or_else(|| {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
                )
                .then(|| {
                    let c_name = data_name_to_c_name(name);
                    find_data_item_size(&c_name, data_items).to_string()
                })
            })
        }
        HirExpr::Subscript { variable, .. } => {
            find_data_item(variable, data_items).and_then(|item| {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. } | HirType::National { .. }
                )
                .then(|| {
                    let c_name = data_name_to_c_name(variable);
                    find_data_item_size(&c_name, data_items).to_string()
                })
            })
        }
        HirExpr::ReferenceModification {
            length, variable, ..
        } => {
            if let Some(len) = length {
                if let HirExpr::Literal(HirLiteral::Integer(n)) = len.as_ref() {
                    Some((*n).max(0).to_string())
                } else {
                    let c_name = data_name_to_c_name(variable);
                    Some(find_data_item_size(&c_name, data_items).to_string())
                }
            } else {
                let c_name = data_name_to_c_name(variable);
                Some(find_data_item_size(&c_name, data_items).to_string())
            }
        }
        HirExpr::Literal(HirLiteral::String(s)) => Some(s.len().to_string()),
        HirExpr::Literal(HirLiteral::Space)
        | HirExpr::Literal(HirLiteral::HighValue)
        | HirExpr::Literal(HirLiteral::LowValue)
        | HirExpr::Literal(HirLiteral::Quote)
        | HirExpr::Literal(HirLiteral::Zero) => Some("1".to_string()),
        _ => None,
    }
}

fn effective_item_len_c_expr(item: &HirDataItem, data_items: &[HirDataItem]) -> Option<String> {
    let unit_expr = match &item.data_type {
        HirType::Group { members, .. } => effective_group_members_len_c_expr(members, data_items)
            .unwrap_or_else(|| data_item_byte_size(&item.data_type).to_string()),
        _ => data_item_byte_size(&item.data_type).to_string(),
    };

    if let Some(depending) = &item.occurs_depending_on {
        let dep_expr =
            emit_int_compatible_expr(&HirExpr::Variable(depending.clone().into()), data_items);
        Some(format!("((uint32_t)({dep_expr}) * ({unit_expr}))"))
    } else if let Some(count) = item.occurs {
        if group_contains_depending(item) {
            Some(format!("({count} * ({unit_expr}))"))
        } else {
            None
        }
    } else if group_contains_depending(item) {
        Some(unit_expr)
    } else {
        None
    }
}

fn effective_group_members_len_c_expr(
    members: &[HirDataItem],
    data_items: &[HirDataItem],
) -> Option<String> {
    let mut parts = Vec::new();
    let mut dynamic = false;
    for member in members {
        if member.redefines.is_some() || member.renames.is_some() {
            continue;
        }
        if let Some(expr) = effective_item_len_c_expr(member, data_items) {
            parts.push(expr);
            dynamic = true;
        } else {
            let count = member.occurs.unwrap_or(1);
            parts.push((data_item_byte_size(&member.data_type) * count).to_string());
        }
    }
    dynamic.then(|| parts.join(" + "))
}

fn group_contains_depending(item: &HirDataItem) -> bool {
    match &item.data_type {
        HirType::Group { members, .. } => members.iter().any(|member| {
            member.occurs_depending_on.is_some()
                || matches!(member.data_type, HirType::Group { .. })
                    && group_contains_depending(member)
        }),
        _ => false,
    }
}

fn emit_transfer_to_target(
    out: &mut String,
    target: &HirTransferTarget,
    paragraphs: &[HirParagraph],
    pad: &str,
    in_body: bool,
    current_paragraph: Option<HirParagraphId>,
) {
    let c_target = transfer_target_c_name(target, paragraphs);
    let target_name = escape_c_string(target.name());
    let paragraph_label_id = target
        .paragraph_id()
        .and_then(|id| with_active_context(|ctx| ctx.label_id(id)));
    if in_body && paragraph_label_id.is_some() {
        emit_optional_debug_event(out, pad, &target_name, "");
        out.push_str(&format!("{pad}goto lbl_{c_target};\n"));
        return;
    }

    let body_label_id = target
        .paragraph_id()
        .and_then(|id| with_active_context(|ctx| ctx.body_label_id(id)));
    let label_id = paragraph_label_id.or(body_label_id);
    if let Some(id) = label_id {
        emit_optional_debug_event(out, pad, &target_name, "");
        if let Some(target_id) = target.paragraph_id() {
            if should_suppress_segment_reset(target_id, paragraphs, current_paragraph) {
                out.push_str(&format!("{pad}_suppress_segment_reset = 1;\n"));
            } else {
                out.push_str(&format!("{pad}_suppress_segment_reset = 0;\n"));
            }
        }
        if in_body && paragraph_label_id.is_none() && body_label_id.is_some() {
            out.push_str(&format!("{pad}_goto_target = {id}; return;\n"));
        } else {
            out.push_str(&format!("{pad}_goto_target = {id}; goto _goto_dispatch;\n"));
        }
    } else {
        emit_optional_debug_event(out, pad, &target_name, "");
        out.push_str(&format!("{pad}para_{c_target}(); return;\n"));
    }
}

fn emit_alterable_goto_dispatch(
    out: &mut String,
    info: &AlterableParagraphInfo,
    paragraphs: &[HirParagraph],
    pad: &str,
    in_body: bool,
    current_paragraph: Option<HirParagraphId>,
) {
    out.push_str(&format!("{pad}switch ({}) {{\n", info.dispatch_var));
    for target in &info.targets {
        let Some(target_id) = target.paragraph_id() else {
            continue;
        };
        out.push_str(&format!("{pad}    case {}:\n", target_id.0));
        emit_transfer_to_target(
            out,
            target,
            paragraphs,
            &format!("{pad}        "),
            in_body,
            current_paragraph,
        );
        out.push_str(&format!("{pad}        break;\n"));
    }
    out.push_str(&format!("{pad}    default: break;\n"));
    out.push_str(&format!("{pad}}}\n"));
}

fn resolve_record_key_item(
    key_name: &str,
    record_var: &str,
    data_items: &[HirDataItem],
) -> Option<(String, u32, bool)> {
    let target = sanitize_name(key_name);
    let record = data_items
        .iter()
        .find(|item| sanitize_name(&item.name) == record_var)?;
    let mut matches = Vec::new();
    let mut qualifiers = vec![sanitize_name(&record.name)];
    collect_record_key_item_matches(record, &target, &mut qualifiers, &mut matches);
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

fn collect_record_key_item_matches(
    item: &HirDataItem,
    target: &str,
    qualifiers: &mut Vec<String>,
    matches: &mut Vec<(String, u32, bool)>,
) {
    let HirType::Group { members, .. } = &item.data_type else {
        return;
    };
    let mut member_name_counts = HashMap::new();
    for member in members {
        let c_name = dedup_record_member_context_name(member, &mut member_name_counts);
        qualifiers.push(c_name.clone());
        if c_name == target {
            let is_group = matches!(member.data_type, HirType::Group { .. });
            matches.push((
                qualifiers.join("__"),
                data_item_storage_size(member),
                is_group,
            ));
        }
        collect_record_key_item_matches(member, target, qualifiers, matches);
        qualifiers.pop();
    }
}

fn dedup_record_member_context_name(
    member: &HirDataItem,
    member_name_counts: &mut HashMap<String, u32>,
) -> String {
    let base_c_name = sanitize_name(&member.name);
    let count = member_name_counts.entry(base_c_name.clone()).or_insert(0);
    *count += 1;
    if *count > 1 {
        format!("{}_{}", base_c_name, count)
    } else {
        base_c_name
    }
}

fn emit_optional_debug_event(out: &mut String, pad: &str, name: &str, contents: &str) {
    if !should_emit_debug_events() {
        return;
    }
    out.push_str(&format!(
        "{pad}_set_debug_event(\"{name}\", \"{contents}\", \"\");\n"
    ));
}

fn should_emit_debug_events() -> bool {
    with_active_context(|ctx| ctx.has_debug_declaratives() && !ctx.in_debug_declarative())
}

fn emit_debug_raw_contents_event(
    out: &mut String,
    pad: &str,
    name: &str,
    ptr_expr: &str,
    len_expr: &str,
    condition: Option<&str>,
    dispatch_reference: bool,
) {
    if !should_emit_debug_events() {
        return;
    }
    let event_name = escape_c_string(name);
    out.push_str(&format!("{pad}{{\n"));
    if let Some(condition) = condition {
        out.push_str(&format!("{pad}    if ({condition}) {{\n"));
        out.push_str(&format!(
            "{pad}        size_t _debug_ref_len = ({len_expr}) < (sizeof(_debug_event_contents) - 1) ? ({len_expr}) : (sizeof(_debug_event_contents) - 1);\n"
        ));
        out.push_str(&format!("{pad}        _debug_event_explicit = 1;\n"));
        out.push_str(&format!(
            "{pad}        _debug_copy_text_field(_debug_event_name, sizeof(_debug_event_name), \"{event_name}\");\n"
        ));
        out.push_str(&format!(
            "{pad}        memset(_debug_event_contents, ' ', sizeof(_debug_event_contents));\n"
        ));
        out.push_str(&format!(
            "{pad}        memcpy(_debug_event_contents, (const uint8_t*){ptr_expr}, _debug_ref_len);\n"
        ));
        out.push_str(&format!(
            "{pad}        _debug_event_contents[sizeof(_debug_event_contents) - 1] = '\\0';\n"
        ));
        out.push_str(&format!(
            "{pad}        _debug_copy_text_field(_debug_event_line, sizeof(_debug_event_line), \"\");\n"
        ));
        out.push_str(&format!(
            "{pad}        _dispatch_debug_declarative(\"{event_name}\");\n"
        ));
        if dispatch_reference {
            out.push_str(&format!(
                "{pad}        _dispatch_debug_reference(\"{event_name}\");\n"
            ));
        }
        out.push_str(&format!("{pad}    }}\n"));
    } else {
        out.push_str(&format!(
            "{pad}    size_t _debug_ref_len = ({len_expr}) < (sizeof(_debug_event_contents) - 1) ? ({len_expr}) : (sizeof(_debug_event_contents) - 1);\n"
        ));
        out.push_str(&format!("{pad}    _debug_event_explicit = 1;\n"));
        out.push_str(&format!(
            "{pad}    _debug_copy_text_field(_debug_event_name, sizeof(_debug_event_name), \"{event_name}\");\n"
        ));
        out.push_str(&format!(
            "{pad}    memset(_debug_event_contents, ' ', sizeof(_debug_event_contents));\n"
        ));
        out.push_str(&format!(
            "{pad}    memcpy(_debug_event_contents, (const uint8_t*){ptr_expr}, _debug_ref_len);\n"
        ));
        out.push_str(&format!(
            "{pad}    _debug_event_contents[sizeof(_debug_event_contents) - 1] = '\\0';\n"
        ));
        out.push_str(&format!(
            "{pad}    _debug_copy_text_field(_debug_event_line, sizeof(_debug_event_line), \"\");\n"
        ));
        out.push_str(&format!(
            "{pad}    _dispatch_debug_declarative(\"{event_name}\");\n"
        ));
        if dispatch_reference {
            out.push_str(&format!(
                "{pad}    _dispatch_debug_reference(\"{event_name}\");\n"
            ));
        }
    }
    out.push_str(&format!("{pad}}}\n"));
}

fn emit_debug_spaces_event(out: &mut String, pad: &str, name: &str) {
    if !should_emit_debug_events() {
        return;
    }
    let event_name = escape_c_string(name);
    out.push_str(&format!("{pad}{{\n"));
    out.push_str(&format!(
        "{pad}    _set_debug_event(\"{event_name}\", \"\", \"\");\n"
    ));
    out.push_str(&format!(
        "{pad}    _dispatch_debug_declarative(\"{event_name}\");\n"
    ));
    out.push_str(&format!("{pad}}}\n"));
}

#[allow(clippy::too_many_arguments)]
fn emit_debug_data_name_event(
    out: &mut String,
    pad: &str,
    event_name: &str,
    data_name: &str,
    data_items: &[HirDataItem],
    condition: Option<&str>,
    dispatch_reference: bool,
    serialize_group: bool,
) {
    let c_name = sanitize_name(data_name);
    let size = find_data_item_storage_size(&c_name, data_items);
    if size == 0 {
        emit_debug_spaces_event(out, pad, event_name);
        return;
    }
    if serialize_group
        && find_data_item(&c_name, data_items)
            .is_some_and(|item| matches!(item.data_type, HirType::Group { .. }))
    {
        out.push_str(&format!("{pad}{{\n"));
        out.push_str(&format!(
            "{pad}    uint8_t _debug_group_contents[{size}];\n"
        ));
        out.push_str(&format!(
            "{pad}    memset(_debug_group_contents, ' ', {size});\n"
        ));
        emit_sort_record_serialize(
            out,
            &c_name,
            data_items,
            "_debug_group_contents",
            &format!("{pad}    "),
        );
        emit_debug_raw_contents_event(
            out,
            &format!("{pad}    "),
            event_name,
            "_debug_group_contents",
            &size.to_string(),
            condition,
            dispatch_reference,
        );
        out.push_str(&format!("{pad}}}\n"));
        return;
    }
    let ptr = c_ptr_expr(&c_name, data_items);
    emit_debug_raw_contents_event(
        out,
        pad,
        event_name,
        &ptr,
        &size.to_string(),
        condition,
        dispatch_reference,
    );
}

fn emit_debug_communication_event(
    out: &mut String,
    pad: &str,
    target_name: &str,
    binding: Option<&CommunicationBinding>,
    data_items: &[HirDataItem],
    condition: Option<&str>,
) {
    if let Some(record_name) = binding.and_then(|binding| binding.record_name.as_deref()) {
        emit_debug_data_name_event(
            out,
            pad,
            target_name,
            record_name,
            data_items,
            condition,
            false,
            false,
        );
    } else {
        emit_debug_spaces_event(out, pad, target_name);
    }
}

fn emit_debug_identifier_value_event(
    out: &mut String,
    pad: &str,
    name: &str,
    c_value: &str,
    width: u32,
    data_items: &[HirDataItem],
    include_redefines_declarative: bool,
) {
    if !should_emit_debug_events() {
        return;
    }
    let dispatch_names = if include_redefines_declarative {
        debug_identifier_dispatch_names(name, data_items)
    } else {
        vec![name.to_string()]
    };
    let name_is_display_numeric = c_expr_is_display_numeric(&sanitize_name(name));
    let debug_value = if name_is_display_numeric {
        c_value.to_string()
    } else if find_data_item(name, data_items).is_some_and(|item| needs_decimal(&item.data_type)) {
        decimal_debug_value_expr(c_value)
    } else {
        c_value.to_string()
    };
    let event_name = escape_c_string(name);
    out.push_str(&format!("{pad}{{\n"));
    out.push_str(&format!("{pad}    char _debug_ref_contents[81];\n"));
    if width > 0 {
        out.push_str(&format!(
            "{pad}    snprintf(_debug_ref_contents, sizeof(_debug_ref_contents), \"%0*lld\", {width}, (long long){debug_value});\n"
        ));
    } else {
        out.push_str(&format!(
            "{pad}    snprintf(_debug_ref_contents, sizeof(_debug_ref_contents), \"%lld\", (long long){debug_value});\n"
        ));
    }
    out.push_str(&format!(
        "{pad}    _set_debug_event(\"{event_name}\", _debug_ref_contents, \"\");\n"
    ));
    for dispatch_name in dispatch_names {
        let escaped = escape_c_string(&dispatch_name);
        out.push_str(&format!(
            "{pad}    _dispatch_debug_declarative(\"{escaped}\");\n"
        ));
    }
    out.push_str(&format!(
        "{pad}    _dispatch_debug_reference(\"{event_name}\");\n"
    ));
    out.push_str(&format!("{pad}}}\n"));
}

fn debug_identifier_dispatch_names(name: &str, data_items: &[HirDataItem]) -> Vec<String> {
    let mut names = vec![name.to_string()];
    if let Some(item) = find_data_item(name, data_items) {
        if let Some(redefines) = &item.redefines {
            if !names.iter().any(|candidate| candidate == redefines) {
                names.push(redefines.to_string());
            }
        }
    }
    names
}

fn decimal_debug_value_expr(c_expr: &str) -> String {
    let trimmed = c_expr.trim();
    if c_expr_already_decimal_value(trimmed) {
        trimmed.to_string()
    } else {
        format!("({trimmed}).value")
    }
}

fn c_expr_already_decimal_value(c_expr: &str) -> bool {
    let mut trimmed = c_expr.trim();
    while trimmed.starts_with('(') && trimmed.ends_with(')') && c_expr_has_outer_parens(trimmed) {
        trimmed = trimmed[1..trimmed.len() - 1].trim();
    }
    trimmed.ends_with(".value")
}

fn c_expr_has_outer_parens(c_expr: &str) -> bool {
    let mut depth = 0i32;
    for (idx, ch) in c_expr.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && idx + ch.len_utf8() != c_expr.len() {
                    return false;
                }
            }
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

fn emit_debug_numeric_identifier_target_event(
    out: &mut String,
    pad: &str,
    name: &HirDataName,
    c_target: &str,
    data_items: &[HirDataItem],
) {
    let Some(item) = find_data_item_by_name(name, data_items) else {
        return;
    };
    match item.data_type {
        HirType::Numeric { size, .. } => {
            let target_expr =
                if expr_name_is_display_numeric(name) || c_expr_is_display_numeric(c_target) {
                    format!(
                        "cobol_display_to_int64({}, {size})",
                        display_numeric_const_ptr(c_target)
                    )
                } else if needs_decimal(&item.data_type) {
                    decimal_debug_value_expr(c_target)
                } else {
                    c_target.to_string()
                };
            emit_debug_identifier_value_event(
                out,
                pad,
                name.name.as_str(),
                &target_expr,
                size,
                data_items,
                false,
            );
        }
        HirType::Alphanumeric { .. } | HirType::Group { .. } => {
            if !should_emit_debug_events() {
                return;
            }
            let event_name = escape_c_string(&debug_data_display_name(name));
            let declarative_name = escape_c_string(name.name.as_str());
            let reference_name = escape_c_string(&debug_data_display_name(name));
            let size = find_data_item_storage_size(&data_name_to_c_name(name), data_items);
            let ptr = c_ptr_expr(c_target, data_items);
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!("{pad}    char _debug_ref_contents[81];\n"));
            out.push_str(&format!(
                "{pad}    size_t _debug_ref_len = {size} < 80 ? {size} : 80;\n"
            ));
            out.push_str(&format!(
                "{pad}    memcpy(_debug_ref_contents, (const uint8_t*){ptr}, _debug_ref_len);\n"
            ));
            out.push_str(&format!(
                "{pad}    _debug_ref_contents[_debug_ref_len] = '\\0';\n"
            ));
            out.push_str(&format!(
                "{pad}    _set_debug_event(\"{event_name}\", _debug_ref_contents, \"\");\n"
            ));
            out.push_str(&format!(
                "{pad}    _dispatch_debug_declarative(\"{declarative_name}\");\n"
            ));
            out.push_str(&format!(
                "{pad}    _dispatch_debug_reference(\"{reference_name}\");\n"
            ));
            out.push_str(&format!("{pad}}}\n"));
        }
        _ => {}
    }
}

fn debug_data_display_name(name: &HirDataName) -> String {
    if name.qualifiers.is_empty() {
        return name.name.to_string();
    }
    let mut display = name.name.to_string();
    for qualifier in name
        .qualifiers
        .iter()
        .filter(|qualifier| !qualifier.ends_with("-GROUP"))
    {
        display.push_str(" OF ");
        display.push_str(qualifier);
    }
    display
}

fn emit_debug_numeric_identifier_source_event(
    out: &mut String,
    pad: &str,
    expr: &HirExpr,
    data_items: &[HirDataItem],
) {
    let Some(name) = expr_data_name(expr) else {
        return;
    };
    let Some(item) = find_data_item_by_name(name, data_items) else {
        return;
    };
    let HirType::Numeric { size, .. } = item.data_type else {
        return;
    };
    let c_name = data_name_to_c_name(name);
    let source_expr = if expr_name_is_display_numeric(name) || c_expr_is_display_numeric(&c_name) {
        let c_expr = emit_expr(expr);
        format!(
            "cobol_display_to_int64({}, {size})",
            display_numeric_const_ptr(&c_expr)
        )
    } else if needs_decimal(&item.data_type) {
        decimal_debug_value_expr(&emit_expr(expr))
    } else {
        emit_expr(expr)
    };
    if !should_emit_debug_events() {
        return;
    }
    let event_name = escape_c_string(&debug_data_display_name(name));
    out.push_str(&format!("{pad}{{\n"));
    out.push_str(&format!("{pad}    char _debug_ref_contents[81];\n"));
    out.push_str(&format!(
        "{pad}    snprintf(_debug_ref_contents, sizeof(_debug_ref_contents), \"%0*lld\", {size}, (long long){source_expr});\n"
    ));
    out.push_str(&format!(
        "{pad}    _set_debug_event(\"{event_name}\", _debug_ref_contents, \"\");\n"
    ));
    out.push_str(&format!(
        "{pad}    _dispatch_debug_reference(\"{event_name}\");\n"
    ));
    out.push_str(&format!("{pad}}}\n"));
}

fn emit_debug_numeric_unique_source_events(
    out: &mut String,
    pad: &str,
    exprs: &[HirExpr],
    data_items: &[HirDataItem],
    excluded_names: &[String],
) {
    let mut seen = Vec::new();
    for expr in exprs {
        let Some(name) = expr_data_name(expr) else {
            continue;
        };
        if excluded_names
            .iter()
            .any(|excluded_name| excluded_name == name.name)
        {
            continue;
        }
        if seen.iter().any(|seen_name: &String| seen_name == name.name) {
            continue;
        }
        seen.push(name.name.to_string());
        emit_debug_numeric_identifier_source_event(out, pad, expr, data_items);
    }
}

fn emit_debug_subscript_values(
    out: &mut String,
    pad: &str,
    subscripts: &[HirExpr],
    data_items: &[HirDataItem],
) {
    if !should_emit_debug_events() {
        return;
    }
    for (idx, subscript) in subscripts.iter().take(3).enumerate() {
        let c_expr = emit_int_compatible_expr(subscript, data_items);
        out.push_str(&format!("{pad}memset(DEBUG_SUB_{}, ' ', 80);\n", idx + 1));
        out.push_str(&format!(
            "{pad}snprintf(DEBUG_SUB_{}, 6, \"%04lld \", (long long)({c_expr}));\n",
            idx + 1
        ));
    }
}

fn emit_fallthrough_debug_event(out: &mut String, pad: &str, name: &str, contents: &str) {
    if !should_emit_debug_events() {
        return;
    }
    out.push_str(&format!(
        "{pad}_set_fallthrough_debug_event(\"{name}\", \"{contents}\", \"\");\n"
    ));
}

fn emit_display_numeric_storage(
    out: &mut String,
    c_expr: &str,
    item: Option<&HirDataItem>,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let Some((size, scale, is_signed)) = display_numeric_c_expr_metadata(c_expr, data_items) else {
        return;
    };
    let c_ptr = display_numeric_const_ptr(c_expr);
    if scale == 0 {
        out.push_str(&format!(
            "{pad}cobol_display_int(cobol_display_to_int64({c_ptr}, {size}));\n"
        ));
        return;
    }
    let pic_str = item
        .map(|i| generate_pic_string(&i.data_type))
        .unwrap_or_else(|| "9".repeat(size as usize));
    let pic_len = pic_str.len();
    let signed = if is_signed { "true" } else { "false" };
    out.push_str(&format!(
        "{pad}{{ CobolDecimal _display_dec; cobol_decimal_from_int(cobol_display_to_int64({c_ptr}, {size}), {scale}, &_display_dec); _display_dec.size = {size}; _display_dec.scale = {scale}; _display_dec.is_signed = {signed}; char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&_display_dec, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
    ));
}

pub(crate) fn emit_display_operand(
    out: &mut String,
    expr: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match expr {
        HirExpr::DataRef(data_ref) => {
            let c_expr = emit_data_ref_expr(data_ref);
            let item = find_data_item_by_name(&data_ref.name, data_items);
            let is_alphanumeric =
                item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_group = item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            let is_decimal = item.is_some_and(|i| needs_decimal(&i.data_type));

            if display_numeric_c_expr_metadata(&c_expr, data_items).is_some() {
                emit_display_numeric_storage(out, &c_expr, item, data_items, pad);
            } else if is_decimal {
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_expr}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else if is_group {
                let size = match &item.unwrap().data_type {
                    HirType::Group { size, .. } => *size,
                    _ => 1,
                };
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*)&{c_expr}, {size});\n"
                ));
            } else if is_alphanumeric {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::Alphanumeric { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*){c_expr}, {size});\n"
                ));
            } else if item.is_some_and(|i| matches!(i.data_type, HirType::National { .. })) {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::National { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_national((const uint16_t*){c_expr}, {size});\n"
                ));
            } else {
                let e = emit_int_compatible_expr(expr, data_items);
                out.push_str(&format!("{pad}cobol_display_int({e});\n"));
            }
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            out.push_str(&format!("{pad}cobol_display_int({n});\n"));
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            out.push_str(&format!("{pad}cobol_display_int(0);\n"));
        }
        HirExpr::Literal(HirLiteral::Space) => {
            out.push_str(&format!("{pad}cobol_display_space();\n"));
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let escaped = escape_c_string(d);
            let len = d.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\xFF\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\x00\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\\"\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Null) => {
            out.push_str(&format!("{pad}cobol_display_int(0);\n"));
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            let item = find_data_item_by_name(name, data_items);

            // If this is a screen item, emit positioning and attribute code
            if let Some(si) = item.and_then(|i| i.screen_info.as_ref()) {
                emit_screen_display(out, si, data_items, pad);
                // After screen attributes, also display children recursively
                // by emitting the screen group content. For leaf items with a
                // VALUE, the value was already emitted by emit_screen_display.
                return;
            }

            let is_alphanumeric =
                item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_group = item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            let is_decimal = item.is_some_and(|i| needs_decimal(&i.data_type));
            if display_numeric_c_expr_metadata(&c_name, data_items).is_some() {
                emit_display_numeric_storage(out, &c_name, item, data_items, pad);
            } else if is_decimal {
                // Display decimal using cobol_decimal_to_display
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_name}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else if is_group {
                // Group items are C unions; display their raw bytes
                let size = match &item.unwrap().data_type {
                    HirType::Group { size, .. } => *size,
                    _ => 1,
                };
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*)&{c_name}, {size});\n"
                ));
            } else if is_alphanumeric {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::Alphanumeric { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*){c_name}, {size});\n"
                ));
            } else if item.is_some_and(|i| matches!(i.data_type, HirType::National { .. })) {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::National { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_national((const uint16_t*){c_name}, {size});\n"
                ));
            } else {
                let e = emit_int_compatible_expr(expr, data_items);
                out.push_str(&format!("{pad}cobol_display_int({e});\n"));
            }
        }
        HirExpr::BinaryOp { .. } | HirExpr::UnaryOp { .. } => {
            let e = emit_int_compatible_expr(expr, data_items);
            out.push_str(&format!("{pad}cobol_display_int({e});\n"));
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_name = name.to_uppercase();
            match upper_name.as_str() {
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    // In-place string functions: copy arg to temp buffer,
                    // apply function, then display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let func = match upper_name.as_str() {
                            "UPPER-CASE" => "cobol_func_upper_case",
                            "LOWER-CASE" => "cobol_func_lower_case",
                            _ => "cobol_func_reverse",
                        };
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _fbuf[{size}]; memcpy(_fbuf, (const uint8_t*){c_arg}, {size}); {func}(_fbuf, {size}); cobol_display_string(_fbuf, {size}); }}\n"
                        ));
                    }
                }
                "TRIM" => {
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        // mode: 0 = both, 1 = leading, 2 = trailing
                        let mode = if args.len() > 1 {
                            emit_expr(&args[1])
                        } else {
                            "0".to_string()
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _fbuf[256]; uint32_t _flen = cobol_func_trim((const uint8_t*){c_arg}, {size}, _fbuf, 256, {mode}); cobol_display_string(_fbuf, _flen); }}\n"
                        ));
                    }
                }
                "CONCATENATE" => {
                    // For display: concatenate all args into a temp buffer
                    // and display the result
                    let mut total_size = 0u32;
                    let mut arg_parts: Vec<(String, u32)> = Vec::new();
                    for arg in args {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else if let HirExpr::Literal(HirLiteral::String(s)) = arg {
                            s.len() as u32
                        } else {
                            64
                        };
                        total_size += size;
                        arg_parts.push((c_arg, size));
                    }
                    if !arg_parts.is_empty() {
                        let buf_size = total_size.max(1);
                        let mut block =
                            format!("{pad}{{ uint8_t _cbuf[{buf_size}]; uint32_t _coff = 0;\n");
                        for (c_arg, size) in &arg_parts {
                            block.push_str(&format!(
                                "{pad}  memcpy(_cbuf + _coff, \
                                 (const uint8_t*){c_arg}, {size}); \
                                 _coff += {size};\n"
                            ));
                        }
                        block.push_str(&format!(
                            "{pad}  cobol_display_string(_cbuf, {buf_size}); }}\n"
                        ));
                        out.push_str(&block);
                    }
                }
                "NATIONAL-OF" => {
                    // DISPLAY FUNCTION NATIONAL-OF(var) -- convert and display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint16_t _nbuf[{size}]; \
                             cobol_func_national_of(\
                             (const uint8_t*){c_arg}, {size}, _nbuf, {size}); \
                             cobol_display_national(_nbuf, {size}); }}\n"
                        ));
                    }
                }
                "DISPLAY-OF" => {
                    // DISPLAY FUNCTION DISPLAY-OF(var) -- convert and display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _dbuf[{size}]; \
                             cobol_func_display_of(\
                             (const uint16_t*){c_arg}, {size}, _dbuf, {size}); \
                             cobol_display_string(_dbuf, {size}); }}\n"
                        ));
                    }
                }
                _ => {
                    // Numeric function
                    let e = emit_expr(expr);
                    out.push_str(&format!("{pad}cobol_display_int({e});\n"));
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } => {
            let c_var = data_name_to_c_name(variable);
            let c_start = emit_expr(start);
            let var_size = find_data_item_layout(&c_var, data_items).item_len;
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({var_size} - ({c_start} - 1))")
            };
            out.push_str(&format!(
                "{pad}cobol_display_string(\
                 (const uint8_t*){c_var} + ({c_start} - 1), {c_len});\n"
            ));
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            let c_access = emit_subscript_access(variable, subscripts);
            let item = find_data_item_by_name(variable, data_items);
            let is_alpha =
                item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_decimal = item.is_some_and(|i| needs_decimal(&i.data_type));
            if is_alpha {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::Alphanumeric { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*){c_access}, {size});\n"
                ));
            } else if display_numeric_c_expr_metadata(&c_access, data_items).is_some() {
                emit_display_numeric_storage(out, &c_access, item, data_items, pad);
            } else if is_decimal {
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_access}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else {
                out.push_str(&format!("{pad}cobol_display_int({c_access});\n"));
            }
        }
    }
}

/// Emit C code for displaying a screen item with ANSI positioning and attributes.
pub(crate) fn emit_screen_display(
    out: &mut String,
    si: &cobol_hir::HirScreenInfo,
    data_items: &[HirDataItem],
    pad: &str,
) {
    // BLANK SCREEN: clear the whole terminal
    if si.blank_screen {
        out.push_str(&format!("{pad}cobol_screen_clear();\n"));
    }
    // BLANK LINE: clear current line
    if si.blank_line {
        out.push_str(&format!("{pad}cobol_screen_clear_line();\n"));
    }
    // LINE / COLUMN: position cursor
    if si.line.is_some() || si.column.is_some() {
        let line = si.line.unwrap_or(1) as i32;
        let col = si.column.unwrap_or(1) as i32;
        out.push_str(&format!("{pad}cobol_screen_position({line}, {col});\n"));
    }
    // HIGHLIGHT: enable bold
    if si.highlight {
        out.push_str(&format!("{pad}cobol_screen_highlight_on();\n"));
    }
    // REVERSE-VIDEO
    if si.reverse_video {
        out.push_str(&format!("{pad}cobol_screen_reverse_on();\n"));
    }
    // Display the VALUE if present
    if let Some(ref val) = si.value {
        let escaped = escape_c_string(val);
        let len = val.len();
        out.push_str(&format!(
            "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
        ));
    }
    // Display the SOURCE/USING field if present. USING is both an input and
    // output binding, so DISPLAY treats it as the visible field value.
    if let Some(source) = si.source.as_ref().or(si.using_field.as_ref()) {
        let c_name = sanitize_name(source);
        let item = find_data_item(source, data_items);
        let is_alpha = item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
        if is_alpha {
            let size = item
                .and_then(|i| match &i.data_type {
                    HirType::Alphanumeric { size } => Some(*size),
                    _ => None,
                })
                .unwrap_or(1);
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*){c_name}, {size});\n"
            ));
        } else if let Some(disp_size) = grp_display_size(&c_name, data_items) {
            let c_name_ptr = display_numeric_const_ptr(&c_name);
            out.push_str(&format!(
                "{pad}cobol_display_int(cobol_display_to_int64(\
                 {c_name_ptr}, {disp_size}));\n"
            ));
        } else {
            out.push_str(&format!("{pad}cobol_display_int({c_name});\n"));
        }
    }
    // Reset attributes if we turned any on
    if si.highlight || si.reverse_video {
        out.push_str(&format!("{pad}cobol_screen_reset_attrs();\n"));
    }
}

fn emit_screen_accept(out: &mut String, item: &HirDataItem, data_items: &[HirDataItem], pad: &str) {
    if let Some(si) = item.screen_info.as_ref() {
        emit_screen_display(out, si, data_items, pad);
        if let Some(using) = si.using_field.as_ref() {
            emit_screen_accept_field(out, si, using, data_items, pad);
        }
    }

    if let HirType::Group { members, .. } = &item.data_type {
        for member in members {
            emit_screen_accept(out, member, data_items, pad);
        }
    }
}

fn emit_screen_accept_field(
    out: &mut String,
    si: &cobol_hir::HirScreenInfo,
    using: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if si.line.is_some() || si.column.is_some() {
        let line = si.line.unwrap_or(1) as i32;
        let col = si.column.unwrap_or(1) as i32;
        out.push_str(&format!("{pad}cobol_screen_position({line}, {col});\n"));
    }

    let c_name = sanitize_name(using);
    let item = find_data_item(using, data_items);
    let size = item
        .map(|item| data_item_byte_size(&item.data_type))
        .unwrap_or_else(|| find_data_item_size(&c_name, data_items));

    if display_numeric_c_expr_metadata(&c_name, data_items).is_some()
        || item.is_some_and(|item| matches!(item.data_type, HirType::Alphanumeric { .. }))
    {
        let ptr = c_ptr_expr(&c_name, data_items);
        out.push_str(&format!(
            "{pad}cobol_screen_accept((uint8_t*){ptr}, {size});\n"
        ));
    } else {
        out.push_str(&format!("{pad}{{ uint8_t _screen_buf[64];\n"));
        out.push_str(&format!(
            "{pad}    uint32_t _screen_len = cobol_screen_accept(_screen_buf, 63);\n"
        ));
        out.push_str(&format!("{pad}    _screen_buf[_screen_len] = 0;\n"));
        emit_store_int(
            out,
            &c_name,
            "cobol_display_to_int64((const uint8_t*)_screen_buf, _screen_len)",
            data_items,
            &format!("{pad}    "),
        );
        out.push_str(&format!("{pad}}}\n"));
    }
}

pub(crate) fn emit_move_to(
    out: &mut String,
    from: &HirExpr,
    target_name: &HirDataName,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_c_name = data_name_to_c_name(target_name);
    let target_item = find_data_item_by_name(target_name, data_items)
        .or_else(|| find_data_item_by_c_name(c_target, data_items));
    let target_effective_item =
        target_item.map(|item| resolve_single_renames_source_item(item, data_items));
    let target_type = target_effective_item.map(|item| &item.data_type);
    let inherited_target_alpha = with_active_context(|ctx| ctx.is_group_alpha_name(&target_c_name));
    let inherited_target_group = with_active_context(|ctx| ctx.is_group_name(&target_c_name));
    let is_target_alpha =
        matches!(target_type, Some(HirType::Alphanumeric { .. })) || inherited_target_alpha;
    let is_target_group = matches!(target_type, Some(HirType::Group { .. }))
        || inherited_target_group
        || is_group_item_c(c_target, data_items);
    // JUSTIFIED RIGHT: use right-justified move for alphanumeric targets
    let move_fn = if with_active_context(|ctx| ctx.is_justified_name(&target_c_name)) {
        "cobol_move_string_right"
    } else {
        "cobol_move_string"
    };
    let is_target_national = matches!(target_type, Some(HirType::National { .. }));
    let target_is_display_numeric =
        expr_name_is_display_numeric(target_name) || c_expr_is_display_numeric(c_target);
    let is_target_decimal = !target_is_display_numeric
        && (target_type.is_some_and(needs_decimal)
            || with_active_context(|ctx| ctx.is_decimal_name(&target_c_name)));

    if let Some(src_name) = expr_data_name(from) {
        let src_item = find_data_item_by_name(src_name, data_items);
        let source_is_group =
            src_item.is_some_and(|item| matches!(item.data_type, HirType::Group { .. }));
        let target_accepts_group_bytes = target_effective_item.is_some_and(|item| {
            item.is_numeric_edited
                || is_alphanumeric_edited_item(item)
                || matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Numeric { .. }
                )
        });
        if source_is_group && !is_target_group && target_accepts_group_bytes {
            let c_src = match from {
                HirExpr::Variable(name) => data_name_to_c_name(name),
                HirExpr::DataRef(data_ref) => emit_data_ref_expr(data_ref),
                _ => emit_expr(from),
            };
            let src_ptr = format!("(const uint8_t*){}", c_ptr_expr(&c_src, data_items));
            let src_size = find_data_item_storage_size(&c_src, data_items);
            if !is_target_decimal && grp_display_size(c_target, data_items).is_none() {
                if let Some(HirDataItem {
                    data_type: HirType::Numeric { size, .. },
                    is_numeric_edited: false,
                    ..
                }) = target_effective_item
                {
                    let take_size = src_size.min(*size);
                    let numval_expr = format!("cobol_func_numval({src_ptr}, {take_size})");
                    emit_store_int(out, c_target, &numval_expr, data_items, pad);
                    return;
                }
            }
            let tgt_ptr = c_ptr_expr(c_target, data_items);
            let tgt_size = find_data_item_storage_size(c_target, data_items);
            out.push_str(&format!(
                "{pad}{move_fn}({src_ptr}, {src_size}, (uint8_t*){tgt_ptr}, {tgt_size});\n"
            ));
            return;
        }
    }

    if target_effective_item.is_some_and(|item| item.is_numeric_edited)
        && emit_move_to_numeric_edited(
            out,
            from,
            target_effective_item.unwrap(),
            c_target,
            data_items,
            pad,
        )
    {
        return;
    }
    if is_target_alpha
        && target_effective_item.is_some_and(is_alphanumeric_edited_item)
        && emit_move_to_alphanumeric_edited(
            out,
            from,
            target_effective_item.unwrap(),
            c_target,
            data_items,
            pad,
        )
    {
        return;
    }

    // NATIONAL target: convert source to national
    if is_target_national {
        let tgt_size = match target_type {
            Some(HirType::National { size }) => *size,
            _ => 1,
        };
        match from {
            HirExpr::Literal(HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                out.push_str(&format!(
                    "{pad}cobol_move_to_national(\
                     (const uint8_t*)\"{escaped}\", {src_len}, \
                     {c_target}, {tgt_size});\n"
                ));
            }
            HirExpr::Literal(HirLiteral::Space) => {
                out.push_str(&format!(
                    "{pad}for (uint32_t _i = 0; _i < {tgt_size}; _i++) \
                     {{ {c_target}[_i] = 0x0020; }}\n"
                ));
            }
            HirExpr::DataRef(data_ref) => {
                let c_src = emit_data_ref_expr(data_ref);
                let src_item = find_data_item(&data_ref.name, data_items).map(|i| &i.data_type);
                if matches!(src_item, Some(HirType::National { .. })) {
                    let src_size = match src_item {
                        Some(HirType::National { size }) => *size,
                        _ => 1,
                    };
                    out.push_str(&format!(
                        "{pad}cobol_move_national_to_national(\
                         (const uint16_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                } else {
                    let src_size = find_data_item_layout(&c_src, data_items).item_len;
                    out.push_str(&format!(
                        "{pad}cobol_move_to_national(\
                         (const uint8_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                }
            }
            HirExpr::Variable(src_name) => {
                let c_src = data_name_to_c_name(src_name);
                let src_item = find_data_item(src_name, data_items).map(|i| &i.data_type);
                if matches!(src_item, Some(HirType::National { .. })) {
                    let src_size = match src_item {
                        Some(HirType::National { size }) => *size,
                        _ => 1,
                    };
                    out.push_str(&format!(
                        "{pad}cobol_move_national_to_national(\
                         (const uint16_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                } else {
                    let src_size = find_data_item_layout(&c_src, data_items).item_len;
                    out.push_str(&format!(
                        "{pad}cobol_move_to_national(\
                         (const uint8_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                }
            }
            _ => {
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!("{pad}{c_target}[0] = (uint16_t){e};\n"));
            }
        }
        return;
    }

    // Group-to-group move: use logical storage sizes so REDEFINES overlays
    // copy only the backing storage, not the overlay's expanded view.
    if is_target_group {
        if let Some(src_name) = expr_data_name(from) {
            let c_src = match from {
                HirExpr::Variable(name) => data_name_to_c_name(name),
                HirExpr::DataRef(data_ref) => emit_data_ref_expr(data_ref),
                _ => emit_expr(from),
            };
            let src_item = find_data_item_by_name(src_name, data_items)
                .or_else(|| find_data_item(data_name_to_c_name(src_name), data_items));
            let is_source_group = src_item
                .is_some_and(|item| matches!(item.data_type, HirType::Group { .. }))
                || is_group_expr(from, data_items);
            let is_source_alpha_like = src_item.is_some_and(|item| {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::National { .. }
                )
            }) || alphanumeric_expr_len(from, data_items).is_some();
            if is_source_group {
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                let src_sz = alphanumeric_expr_len_expr(from, data_items)
                    .unwrap_or_else(|| find_data_item_storage_size(&c_src, data_items).to_string());
                let tgt_sz = find_data_item_storage_size(c_target, data_items);
                let source_needs_conv = sort_record_needs_conversion(&c_src, data_items);
                let target_needs_conv = sort_record_needs_conversion(c_target, data_items);
                if source_needs_conv || target_needs_conv {
                    let flat_len = src_sz.parse::<u32>().unwrap_or(tgt_sz).max(tgt_sz);
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!("{pad}    uint8_t _move_flat[{flat_len}];\n"));
                    out.push_str(&format!("{pad}    memset(_move_flat, ' ', {flat_len});\n"));
                    if source_needs_conv {
                        emit_sort_record_serialize(
                            out,
                            &c_src,
                            data_items,
                            "_move_flat",
                            &format!("{pad}    "),
                        );
                    } else {
                        let src_ptr = c_ptr_expr(&c_src, data_items);
                        out.push_str(&format!(
                            "{pad}    memcpy(_move_flat, {src_ptr}, {src_sz});\n"
                        ));
                    }
                    if target_needs_conv {
                        emit_sort_record_deserialize(
                            out,
                            c_target,
                            data_items,
                            "_move_flat",
                            &format!("{pad}    "),
                        );
                    } else {
                        out.push_str(&format!(
                            "{pad}    size_t _src_sz = {src_sz};\n\
                             {pad}    size_t _tgt_sz = {tgt_sz};\n\
                             {pad}    size_t _cp_sz = _src_sz < _tgt_sz ? _src_sz : _tgt_sz;\n\
                             {pad}    memcpy({tgt_ptr}, _move_flat, _cp_sz);\n\
                             {pad}    if (_src_sz < _tgt_sz) memset((uint8_t*){tgt_ptr} + _src_sz, ' ', _tgt_sz - _src_sz);\n"
                        ));
                    }
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    let src_ptr = c_ptr_expr(&c_src, data_items);
                    out.push_str(&format!(
                        "{pad}{{\n\
                         {pad}    size_t _src_sz = {src_sz};\n\
                         {pad}    size_t _tgt_sz = {tgt_sz};\n\
                         {pad}    size_t _cp_sz = _src_sz < _tgt_sz ? _src_sz : _tgt_sz;\n\
                         {pad}    memcpy({tgt_ptr}, {src_ptr}, _cp_sz);\n\
                         {pad}    if (_src_sz < _tgt_sz) {{\n\
                         {pad}        memset((uint8_t*){tgt_ptr} + _src_sz, ' ', \
                         _tgt_sz - _src_sz);\n\
                         {pad}    }}\n\
                         {pad}}}\n"
                    ));
                }
            } else if is_source_alpha_like {
                let src_size = alphanumeric_expr_len(from, data_items)
                    .unwrap_or_else(|| find_data_item_layout(&c_src, data_items).item_len);
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let copy_size = src_size.min(tgt_size);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!(
                    "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                         {pad}memcpy({tgt_ptr}, (const uint8_t*){c_src}, {copy_size});\n"
                ));
            } else if let Some(src_size) = grp_display_size(&c_src, data_items) {
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                let src_ptr = display_numeric_const_ptr(&c_src);
                out.push_str(&format!(
                    "{pad}cobol_move_string({src_ptr}, {src_size}, \
                     (uint8_t*){tgt_ptr}, {tgt_size});\n"
                ));
            } else if let Some(item) = src_item.filter(|item| {
                matches!(
                    item.data_type,
                    HirType::Numeric {
                        decimal_places: 0,
                        ..
                    }
                )
            }) {
                let HirType::Numeric { size, .. } = &item.data_type else {
                    unreachable!();
                };
                let c_value = emit_int_compatible_expr(from, data_items);
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!(
                    "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%0*lld\", {size}, (long long)llabs({c_value})); \
                     cobol_move_string((const uint8_t*)_nbuf, (uint32_t)_nlen, (uint8_t*){tgt_ptr}, {tgt_size}); }}\n"
                ));
            } else {
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                if src_item.is_some_and(|i| needs_decimal(&i.data_type)) {
                    let src_size = src_item
                        .and_then(|i| match &i.data_type {
                            HirType::Numeric { size, .. }
                            | HirType::Comp3 { size, .. }
                            | HirType::Binary { size, .. } => Some(*size),
                            _ => None,
                        })
                        .unwrap_or_else(|| find_data_item_storage_size(&c_src, data_items));
                    out.push_str(&format!(
                        "{pad}{{ char _dbuf[64]; int _dlen = snprintf(_dbuf, sizeof(_dbuf), \"%0*lld\", {src_size}, (long long)llabs({c_src}.value)); \
                         cobol_move_string((const uint8_t*)_dbuf, (uint32_t)_dlen, (uint8_t*){tgt_ptr}, {tgt_size}); }}\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_move_numeric_to_display({c_src}, 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                    ));
                }
            }
        } else if let HirExpr::Subscript { variable, .. } = from {
            // Subscripted source to group target: check type and use memcpy
            let c_src = emit_expr(from);
            let src_item = find_data_item(variable, data_items);
            let is_src_alpha =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_src_group =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            if is_src_alpha || is_src_group || alphanumeric_expr_len(from, data_items).is_some() {
                let src_size = alphanumeric_expr_len(from, data_items).unwrap_or_else(|| {
                    find_data_item_layout(&data_name_to_c_name(variable), data_items).item_len
                });
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let copy_size = src_size.min(tgt_size);
                let src_ptr = if is_src_group {
                    c_ptr_expr(&c_src, data_items)
                } else {
                    c_src.clone()
                };
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!(
                    "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                     {pad}memcpy({tgt_ptr}, {src_ptr}, {copy_size});\n"
                ));
            } else {
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                if src_item.is_some_and(|i| needs_decimal(&i.data_type)) {
                    let c_src = emit_expr(from);
                    let pic_str = src_item
                        .map(|i| generate_pic_string(&i.data_type))
                        .unwrap_or_else(|| "9".to_string());
                    let pic_len = pic_str.len();
                    out.push_str(&format!(
                        "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                         {pad}{{ char _dbuf[64]; uint32_t _dlen = \
                         cobol_decimal_to_display(&{c_src}, (uint8_t*)_dbuf, 64, \
                         (const uint8_t*)\"{pic_str}\", {pic_len}); \
                         memcpy({tgt_ptr}, _dbuf, _dlen < {tgt_size} ? _dlen : {tgt_size}); }}\n"
                    ));
                } else {
                    let e = emit_int_compatible_expr(from, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_numeric_to_display({e}, 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                    ));
                }
            }
        } else if let HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } = from
        {
            // Reference-modified source to group target: copy substring
            let c_src = data_name_to_c_name(variable);
            let src_ptr = c_ptr_expr(&c_src, data_items);
            let c_start = emit_expr(start);
            let src_full_size = find_data_item_layout(&c_src, data_items).item_len;
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({src_full_size} - ({c_start} - 1))")
            };
            let tgt_size = find_data_item_storage_size(c_target, data_items);
            let tgt_ptr = c_ptr_expr(c_target, data_items);
            out.push_str(&format!(
                "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                 {pad}memcpy({tgt_ptr}, (const uint8_t*){src_ptr} + ({c_start} - 1), \
                 {c_len} < {tgt_size} ? {c_len} : {tgt_size});\n"
            ));
        } else {
            // Non-variable to group: handle figurative constants
            match from {
                HirExpr::Literal(HirLiteral::Space) => {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!("{pad}memset({tgt_ptr}, ' ', {tgt_size});\n"));
                }
                HirExpr::Literal(HirLiteral::Zero) => {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!("{pad}memset({tgt_ptr}, '0', {tgt_size});\n"));
                }
                HirExpr::Literal(HirLiteral::HighValue) => {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!("{pad}memset({tgt_ptr}, 0xFF, {tgt_size});\n"));
                }
                HirExpr::Literal(HirLiteral::LowValue) => {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!("{pad}memset({tgt_ptr}, 0x00, {tgt_size});\n"));
                }
                HirExpr::Literal(HirLiteral::Quote) => {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!("{pad}memset({tgt_ptr}, '\"', {tgt_size});\n"));
                }
                HirExpr::Literal(HirLiteral::AllChar(s)) => {
                    let escaped = escape_c_string(s);
                    let src_len = s.len().max(1);
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}{{ const uint8_t _all[] = \"{escaped}\"; \
                         for (uint32_t _i = 0; _i < {tgt_size}; _i++) \
                         ((uint8_t*){tgt_ptr})[_i] = _all[_i % {src_len}]; }}\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::String(s)) => {
                    let escaped = escape_c_string(s);
                    let src_len = s.len();
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                         {pad}memcpy({tgt_ptr}, \"{escaped}\", \
                         {src_len} < {tgt_size} ? {src_len} : {tgt_size});\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::Integer(n))
                    if target_item.is_some_and(|item| {
                        matches!(
                            &item.data_type,
                            HirType::Group { members, .. }
                                if members.iter().any(|member| {
                                    matches!(member.data_type, HirType::Numeric { .. })
                                })
                        )
                    }) =>
                {
                    let tgt_ptr = c_ptr_expr(c_target, data_items);
                    let tgt_size = find_data_item_storage_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%0*lld\", {tgt_size}, (long long)llabs({n})); \
                         cobol_move_string((const uint8_t*)_nbuf, (uint32_t)_nlen, (uint8_t*){tgt_ptr}, {tgt_size}); }}\n"
                    ));
                }
                _ => {
                    // Byte-like sources for group targets use category move semantics,
                    // including numeric literals which should be copied as their
                    // display representation rather than raw int64 bytes.
                    if is_alpha_expr(from, data_items)
                        || is_group_expr(from, data_items)
                        || alphanumeric_expr_len(from, data_items).is_some()
                        || matches!(
                            from,
                            HirExpr::Literal(HirLiteral::Integer(_))
                                | HirExpr::Literal(HirLiteral::Zero)
                                | HirExpr::Literal(HirLiteral::Decimal(_))
                        )
                    {
                        let (src_ptr, src_size) = emit_alphanumeric_operand(from, data_items);
                        let tgt_size = find_data_item_storage_size(c_target, data_items);
                        let tgt_ptr = c_ptr_expr(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                             {pad}memcpy({tgt_ptr}, {src_ptr}, ({src_size}) < {tgt_size} ? ({src_size}) : {tgt_size});\n"
                        ));
                    } else {
                        let tgt_ptr = c_ptr_expr(c_target, data_items);
                        let tgt_size = find_data_item_storage_size(c_target, data_items);
                        if let Some(src_item) = expr_data_name(from)
                            .and_then(|name| find_data_item_by_name(name, data_items))
                        {
                            let c_src = emit_expr(from);
                            if needs_decimal(&src_item.data_type) {
                                let src_size = match &src_item.data_type {
                                    HirType::Numeric { size, .. }
                                    | HirType::Comp3 { size, .. }
                                    | HirType::Binary { size, .. } => *size,
                                    _ => find_data_item_storage_size(&c_src, data_items),
                                };
                                out.push_str(&format!(
                                    "{pad}{{ char _dbuf[64]; int _dlen = snprintf(_dbuf, sizeof(_dbuf), \"%0*lld\", {src_size}, (long long)llabs({c_src}.value)); \
                                     cobol_move_string((const uint8_t*)_dbuf, (uint32_t)_dlen, (uint8_t*){tgt_ptr}, {tgt_size}); }}\n"
                                ));
                            } else if let Some(src_size) = grp_display_size(&c_src, data_items) {
                                let src_ptr = display_numeric_const_ptr(&c_src);
                                out.push_str(&format!(
                                    "{pad}cobol_move_string({src_ptr}, {src_size}, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                                ));
                            } else if let HirType::Numeric { size, .. } = &src_item.data_type {
                                if c_src.contains("__") || is_group_member_field(&c_src) {
                                    let src_ptr = c_ptr_expr(&c_src, data_items);
                                    let src_size = find_data_item_storage_size(&c_src, data_items);
                                    out.push_str(&format!(
                                        "{pad}cobol_move_string((const uint8_t*){src_ptr}, {src_size}, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%0*lld\", {size}, (long long)llabs({c_src})); \
                                         cobol_move_string((const uint8_t*)_nbuf, (uint32_t)_nlen, (uint8_t*){tgt_ptr}, {tgt_size}); }}\n"
                                    ));
                                }
                            } else {
                                let e = emit_int_compatible_expr(from, data_items);
                                out.push_str(&format!(
                                    "{pad}cobol_move_numeric_to_display({e}, 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                                ));
                            }
                        } else if is_decimal_expr(from, data_items) {
                            let e = emit_expr_as_double(from);
                            out.push_str(&format!(
                                "{pad}cobol_move_numeric_to_display((int64_t)({e}), 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                            ));
                        } else {
                            let e = emit_int_compatible_expr(from, data_items);
                            out.push_str(&format!(
                                "{pad}cobol_move_numeric_to_display({e}, 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                            ));
                        }
                    }
                }
            }
        }
        return;
    }

    // CobolDecimal target: use proper conversion functions
    if is_target_decimal {
        emit_assign_to_decimal(out, from, c_target, data_items, pad);
        return;
    }

    // Detect source type for cross-type moves (handles Variable and Subscript)
    let src_data_name = expr_data_name(from);
    let src_var_name = src_data_name.map(HirDataName::as_str).unwrap_or("");
    let src_type = src_data_name
        .and_then(|name| find_data_item(name, data_items))
        .map(|i| &i.data_type);
    let is_source_index =
        src_data_name.is_some() && src_type.is_none() && is_index_name(src_var_name, data_items);
    let is_source_display_numeric_var = matches!(src_type, Some(HirType::Numeric { .. }));
    let is_source_numeric_var = is_source_index
        || matches!(
            src_type,
            Some(
                HirType::Numeric { .. }
                    | HirType::Binary { .. }
                    | HirType::Comp3 { .. }
                    | HirType::Index
            )
        );
    let is_source_decimal_var = src_type.is_some_and(needs_decimal);
    let is_source_alpha_var =
        matches!(src_type, Some(HirType::Alphanumeric { .. })) || is_alpha_expr(from, data_items);
    let is_source_group_var =
        matches!(src_type, Some(HirType::Group { .. })) || is_group_expr(from, data_items);
    let is_source_national_var = matches!(src_type, Some(HirType::National { .. }))
        || src_type.is_some_and(|ty| matches!(ty, HirType::National { .. }));

    // National source -> alphanumeric target: use DISPLAY-OF conversion
    if is_target_alpha && is_source_national_var {
        let (c_src, src_size) = match from {
            HirExpr::DataRef(data_ref) => (
                emit_data_ref_expr(data_ref),
                match find_data_item_by_name(&data_ref.name, data_items).map(|i| &i.data_type) {
                    Some(HirType::National { size }) => *size,
                    _ => 1,
                },
            ),
            HirExpr::Variable(name) => (
                data_name_to_c_name(name),
                match find_data_item_by_name(name, data_items).map(|i| &i.data_type) {
                    Some(HirType::National { size }) => *size,
                    _ => 1,
                },
            ),
            _ => return,
        };
        let tgt_size = find_data_item_layout(c_target, data_items).item_len;
        out.push_str(&format!(
            "{pad}cobol_func_display_of(\
             (const uint16_t*){c_src}, {src_size}, \
             (uint8_t*){c_target}, {tgt_size});\n"
        ));
        if !is_group_member_field(c_target) {
            out.push_str(&format!("{pad}{c_target}[{tgt_size}] = '\\0';\n"));
        }
        return;
    }

    if is_target_alpha {
        if let HirExpr::Literal(HirLiteral::Decimal(d)) = from {
            let escaped = escape_c_string(d);
            let src_len = d.len();
            let tgt_size = find_data_item_layout(c_target, data_items).item_len;
            out.push_str(&format!(
                "{pad}{move_fn}((const uint8_t*)\"{escaped}\", {src_len}, \
                 (uint8_t*){c_target}, {tgt_size});\n"
            ));
            return;
        }
    }

    match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            if is_target_alpha {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                out.push_str(&format!(
                    "{pad}{move_fn}((const uint8_t*)\"{escaped}\", {src_len}, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if is_target_group {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                out.push_str(&format!(
                    "{pad}{move_fn}((const uint8_t*)\"{escaped}\", {src_len}, (uint8_t*)&{c_target}, {tgt_size});\n"
                ));
            } else {
                // Numeric target: parse string as number
                let escaped = escape_c_string(s);
                let src_len = s.len();
                if s.chars().any(|ch| ch == '.') {
                    if let Some((tgt_size, target_scale)) =
                        display_numeric_c_expr_info(c_target, data_items)
                    {
                        let (scaled, scale) = parse_decimal_literal(s);
                        let scaled = fit_scaled_expr_to_display_target(
                            &scaled.to_string(),
                            scale,
                            c_target,
                            tgt_size,
                            target_scale,
                            data_items,
                        );
                        emit_store_display_numeric(
                            out, pad, &scaled, c_target, tgt_size, data_items,
                        );
                        return;
                    }
                }
                let (src_start, parse_len) = target_item
                    .and_then(|item| match item.data_type {
                        HirType::Numeric { size, .. } if src_len > size as usize => {
                            Some((src_len - size as usize, size))
                        }
                        _ => None,
                    })
                    .unwrap_or((0, src_len as u32));
                let numval_expr = format!(
                    "cobol_func_numval((const uint8_t*)\"{escaped}\" + {src_start}, {parse_len})"
                );
                emit_store_int(out, c_target, &numval_expr, data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            if is_target_alpha {
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                if *n == 0 && tgt_size >= 5 {
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*)\"00000\", 5, \
                         (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else {
                    emit_move_numeric_to_alphanumeric(out, &n.to_string(), c_target, tgt_size, pad);
                }
            } else if is_target_group {
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_numeric_to_display({n}, 0, (uint8_t*){tgt_ptr}, {tgt_size});\n"
                ));
            } else if let Some((tgt_size, target_scale)) =
                display_numeric_c_expr_info(c_target, data_items)
            {
                let scaled = fit_scaled_expr_to_display_target(
                    &n.to_string(),
                    0,
                    c_target,
                    tgt_size,
                    target_scale,
                    data_items,
                );
                emit_store_display_numeric(out, pad, &scaled, c_target, tgt_size, data_items);
            } else {
                emit_store_int(out, c_target, &n.to_string(), data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            if let Some((tgt_size, target_scale)) =
                display_numeric_c_expr_info(c_target, data_items)
            {
                let (scaled, scale) = parse_decimal_literal(d);
                let scaled = fit_scaled_expr_to_display_target(
                    &scaled.to_string(),
                    scale,
                    c_target,
                    tgt_size,
                    target_scale,
                    data_items,
                );
                emit_store_display_numeric(out, pad, &scaled, c_target, tgt_size, data_items);
            } else {
                let (scaled, scale) = parse_decimal_literal(d);
                let value = if scale == 0 {
                    scaled.to_string()
                } else {
                    format!("({scaled} / (int64_t)pow(10.0, {scale}))")
                };
                emit_store_int(out, c_target, &value, data_items, pad);
            }
        }
        HirExpr::UnaryOp { .. } if signed_decimal_literal_expr(from).is_some() => {
            let (scaled, scale) = signed_decimal_literal_expr(from).expect("checked is_some");
            if let Some((tgt_size, target_scale)) =
                display_numeric_c_expr_info(c_target, data_items)
            {
                let scaled = fit_scaled_expr_to_display_target(
                    &scaled.to_string(),
                    scale,
                    c_target,
                    tgt_size,
                    target_scale,
                    data_items,
                );
                emit_store_display_numeric(out, pad, &scaled, c_target, tgt_size, data_items);
            } else {
                let value = if scale == 0 {
                    scaled.to_string()
                } else {
                    format!("({scaled} / (int64_t)pow(10.0, {scale}))")
                };
                emit_store_int(out, c_target, &value, data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            if is_target_alpha {
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                if is_group_member_field(c_target) {
                    out.push_str(&format!("{pad}memset({c_target}, '0', {tgt_size});\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}memset({c_target}, '0', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                    ));
                }
            } else if is_target_group {
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!("{pad}memset({tgt_ptr}, '0', {tgt_size});\n"));
            } else if let Some((tgt_size, _)) = display_numeric_c_expr_info(c_target, data_items) {
                emit_store_display_numeric(out, pad, "0", c_target, tgt_size, data_items);
            } else {
                emit_store_int(out, c_target, "0", data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Space) => {
            if is_target_alpha {
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                if is_group_member_field(c_target) {
                    out.push_str(&format!("{pad}memset({c_target}, ' ', {tgt_size});\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}memset({c_target}, ' ', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                    ));
                }
            } else if is_target_group {
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!("{pad}memset({tgt_ptr}, ' ', {tgt_size});\n"));
            } else {
                out.push_str(&format!(
                    "{pad}memset({c_target}, ' ', sizeof({c_target}) - 1);\n"
                ));
                out.push_str(&format!(
                    "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
                ));
            }
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            out.push_str(&format!(
                "{pad}memset({c_target}, 0xFF, sizeof({c_target}) - 1);\n"
            ));
            out.push_str(&format!(
                "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
            ));
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            out.push_str(&format!(
                "{pad}memset({c_target}, 0x00, sizeof({c_target}));\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            let tgt_size = find_data_item_layout(c_target, data_items).item_len;
            if is_group_member_field(c_target) {
                out.push_str(&format!("{pad}memset({c_target}, '\"', {tgt_size});\n"));
            } else {
                out.push_str(&format!(
                    "{pad}memset({c_target}, '\"', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                ));
            }
        }
        HirExpr::Literal(HirLiteral::Null) => {
            emit_store_int(out, c_target, "0", data_items, pad);
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            if is_target_alpha {
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                let grp_member = is_group_member_field(c_target);
                if s.len() == 1 {
                    let ch = s.chars().next().unwrap();
                    if grp_member {
                        out.push_str(&format!("{pad}memset({c_target}, '{ch}', {tgt_size});\n"));
                    } else {
                        out.push_str(&format!(
                            "{pad}memset({c_target}, '{ch}', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                        ));
                    }
                } else {
                    let escaped = escape_c_string(s);
                    let slen = s.len();
                    out.push_str(&format!(
                        "{pad}{{ const char* _all = \"{escaped}\"; int _al = {slen};\n"
                    ));
                    out.push_str(&format!(
                        "{pad}  for (int _i = 0; _i < {tgt_size}; _i++) {c_target}[_i] = _all[_i % _al];\n"
                    ));
                    if grp_member {
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        out.push_str(&format!("{pad}  {c_target}[{tgt_size}] = '\\0'; }}\n"));
                    }
                }
            } else if let Some(ch) = s.chars().next() {
                emit_store_int(out, c_target, &format!("'{ch}'"), data_items, pad);
            }
        }
        _ => {
            // Handle string-returning intrinsic functions in MOVE context
            if let HirExpr::FunctionCall { name, args } = from {
                let upper_fn = name.to_uppercase();
                match upper_fn.as_str() {
                    "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                        if let Some(arg) = args.first() {
                            let func = match upper_fn.as_str() {
                                "UPPER-CASE" => "cobol_func_upper_case",
                                "LOWER-CASE" => "cobol_func_lower_case",
                                _ => "cobol_func_reverse",
                            };
                            let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                            let (c_src, src_size_str) = emit_string_func_arg(arg);
                            out.push_str(&format!(
                                "{pad}{{ uint8_t _fbuf[{src_size_str}]; memcpy(_fbuf, (const uint8_t*){c_src}, {src_size_str}); {func}(_fbuf, {src_size_str}); cobol_move_string(_fbuf, {src_size_str}, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        }
                        return;
                    }
                    "CURRENT-DATE" => {
                        let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _cdbuf[21]; cobol_func_current_date(_cdbuf, 21); cobol_move_string(_cdbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "WHEN-COMPILED" => {
                        let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _wcbuf[21]; cobol_func_when_compiled(_wcbuf, 21); cobol_move_string(_wcbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "CHAR" => {
                        if let Some(arg) = args.first() {
                            let c_arg = emit_expr_as_numeric(arg);
                            let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                            out.push_str(&format!(
                                "{pad}{{ uint8_t _chval = cobol_func_char((uint32_t){c_arg}); cobol_move_string(&_chval, 1, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        }
                        return;
                    }
                    "MAX" | "MIN" => {
                        let has_alpha = args.iter().any(|a| {
                            matches!(a, HirExpr::Literal(HirLiteral::String(_)))
                                || is_alphanumeric_expr(a, data_items)
                        });
                        if has_alpha && is_target_alpha {
                            let func = if upper_fn == "MAX" {
                                "cobol_func_max_alpha"
                            } else {
                                "cobol_func_min_alpha"
                            };
                            let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                            let n = args.len();
                            let mut ptrs = Vec::new();
                            let mut lens = Vec::new();
                            for arg in args {
                                let (c_src, c_len) = emit_string_func_arg(arg);
                                ptrs.push(format!("(const uint8_t*){c_src}"));
                                lens.push(c_len);
                            }
                            let ptrs_init = ptrs.join(", ");
                            let lens_init = lens.join(", ");
                            out.push_str(&format!(
                                "{pad}{{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
                                 uint32_t _al[] = {{{lens_init}}}; \
                                 int32_t _ai = {func}(_ap, _al, {n}); \
                                 cobol_move_string(_ap[_ai], _al[_ai], \
                                 (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                            return;
                        }
                    }
                    _ => {} // Fall through to other handling below
                }
            }
            if is_target_alpha && is_source_decimal_var {
                // CobolDecimal variable → alphanumeric: use cobol_decimal_to_display
                if let Some(name) = expr_data_name(from) {
                    let c_src = emit_expr(from);
                    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                    let src_type = find_data_item(name, data_items).map(|i| &i.data_type);
                    let pic_str = src_type
                        .map(generate_pic_string)
                        .unwrap_or_else(|| "9".to_string());
                    let pic_len = pic_str.len();
                    out.push_str(&format!(
                        "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(\
                         &{c_src}, (uint8_t*)_dbuf, 64, \
                         (const uint8_t*)\"{pic_str}\", {pic_len}); \
                         cobol_move_string((const uint8_t*)_dbuf, _dlen, \
                         (uint8_t*){c_target}, {tgt_size}); }}\n"
                    ));
                }
            } else if is_target_alpha && is_source_display_numeric_var {
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                let c_src = emit_expr(from);
                if let Some(item) =
                    src_data_name.and_then(|name| find_data_item_by_name(name, data_items))
                {
                    let HirType::Numeric {
                        size,
                        decimal_places,
                        ..
                    } = item.data_type
                    else {
                        unreachable!();
                    };
                    if decimal_places == 0 {
                        if let Some(src_size) = grp_display_size(&c_src, data_items) {
                            let src_ptr = display_numeric_const_ptr(&c_src);
                            let raw_value =
                                format!("cobol_display_to_int64({src_ptr}, {src_size})");
                            let value =
                                apply_scale_adjustment_to_read(&raw_value, item.scale_adjustment);
                            let display_width = if item.scale_adjustment > 0 {
                                size + item.scale_adjustment as u32
                            } else {
                                size
                            };
                            out.push_str(&format!(
                                "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%0*lld\", {display_width}, (long long)llabs({value})); \
                                 {move_fn}((const uint8_t*)_nbuf, (uint32_t)_nlen, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        } else {
                            let e = emit_int_compatible_expr(from, data_items);
                            if !emit_move_numeric_item_to_alphanumeric(
                                out, from, &e, c_target, tgt_size, data_items, pad,
                            ) {
                                emit_move_numeric_to_alphanumeric(out, &e, c_target, tgt_size, pad);
                            }
                        }
                    } else {
                        let e = emit_int_compatible_expr(from, data_items);
                        if !emit_move_numeric_item_to_alphanumeric(
                            out, from, &e, c_target, tgt_size, data_items, pad,
                        ) {
                            emit_move_numeric_to_alphanumeric(out, &e, c_target, tgt_size, pad);
                        }
                    }
                } else if let Some(src_size) = grp_display_size(&c_src, data_items) {
                    let src_ptr = display_numeric_const_ptr(&c_src);
                    out.push_str(&format!(
                        "{pad}{move_fn}({src_ptr}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else {
                    let e = emit_int_compatible_expr(from, data_items);
                    if !emit_move_numeric_item_to_alphanumeric(
                        out, from, &e, c_target, tgt_size, data_items, pad,
                    ) {
                        emit_move_numeric_to_alphanumeric(out, &e, c_target, tgt_size, pad);
                    }
                }
            } else if is_target_alpha && is_source_numeric_var {
                let e = emit_int_compatible_expr(from, data_items);
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                if !emit_move_numeric_item_to_alphanumeric(
                    out, from, &e, c_target, tgt_size, data_items, pad,
                ) {
                    emit_move_numeric_to_alphanumeric(out, &e, c_target, tgt_size, pad);
                }
            } else if !is_target_alpha
                && (is_source_alpha_var
                    || (!is_source_numeric_var
                        && !is_source_decimal_var
                        && alphanumeric_expr_len(from, data_items).is_some()))
            {
                // Alphanumeric variable/subscript → numeric: use cobol_func_numval
                let (c_src, src_size) = emit_alphanumeric_operand(from, data_items);
                let numval_expr = format!("cobol_func_numval({c_src}, {src_size})");
                emit_store_int(out, c_target, &numval_expr, data_items, pad);
            } else if is_target_alpha && is_source_group_var {
                // Group source → alphanumeric: copy the backing bytes even when the
                // source is a qualified DataRef instead of a bare variable.
                let c_src = match from {
                    HirExpr::Variable(name) => data_name_to_c_name(name),
                    HirExpr::DataRef(data_ref) => emit_data_ref_expr(data_ref),
                    _ => emit_expr(from),
                };
                let src_size = alphanumeric_expr_len(from, data_items).unwrap_or_else(|| {
                    expr_data_name(from)
                        .map(|name| {
                            find_data_item_layout(&data_name_to_c_name(name), data_items).item_len
                        })
                        .unwrap_or_else(|| find_data_item_layout(&c_src, data_items).item_len)
                });
                let src_ptr = format!("(const uint8_t*)&{c_src}");
                let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                out.push_str(&format!(
                    "{pad}{move_fn}({src_ptr}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if is_target_alpha {
                // Alphanumeric → alphanumeric: use cobol_move_string
                if let HirExpr::Variable(name) = from {
                    let c_src = data_name_to_c_name(name);
                    let src_size = find_data_item_layout(&c_src, data_items).item_len;
                    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                    out.push_str(&format!(
                        "{pad}{move_fn}((const uint8_t*){c_src}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if let HirExpr::ReferenceModification {
                    variable,
                    start,
                    length,
                } = from
                {
                    let c_src = data_name_to_c_name(variable);
                    let c_start = emit_expr(start);
                    let src_full_size = find_data_item_layout(&c_src, data_items).item_len;
                    let c_len = if let Some(len) = length {
                        emit_expr(len)
                    } else {
                        format!("({src_full_size} - ({c_start} - 1))")
                    };
                    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                    out.push_str(&format!(
                        "{pad}{move_fn}((const uint8_t*){c_src} + ({c_start} - 1), {c_len}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if is_source_alpha_var
                    || is_source_group_var
                    || alphanumeric_expr_len(from, data_items).is_some()
                {
                    // Alphanumeric/group-like source including qualified REDEFINES paths.
                    let (src_ptr, src_size) = emit_alphanumeric_operand(from, data_items);
                    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                    out.push_str(&format!(
                        "{pad}{move_fn}({src_ptr}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else {
                    let e = emit_int_compatible_expr(from, data_items);
                    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
                    emit_move_numeric_to_alphanumeric(out, &e, c_target, tgt_size, pad);
                }
            } else if is_source_group_var {
                // Group variable → numeric target: treat group as alphanumeric bytes
                // and convert via cobol_func_numval (group is a C union).
                if let Some(name) = expr_data_name(from) {
                    let c_src = emit_expr(from);
                    let src_size =
                        find_data_item_layout(&data_name_to_c_name(name), data_items).item_len;
                    let src_ptr = if matches!(from, HirExpr::Variable(_)) {
                        format!("(const uint8_t*)&{c_src}")
                    } else {
                        format!("(const uint8_t*){c_src}")
                    };
                    if is_target_decimal {
                        // Target is CobolDecimal
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int(\
                             cobol_func_numval({src_ptr}, {src_size}), \
                             0, &{c_target});\n"
                        ));
                    } else {
                        let numval_expr = format!("cobol_func_numval({src_ptr}, {src_size})");
                        emit_store_int(out, c_target, &numval_expr, data_items, pad);
                    }
                }
            } else if is_source_decimal_var {
                let e = emit_expr(from);
                if let Some((target_size, target_scale)) =
                    display_numeric_c_expr_info(c_target, data_items)
                {
                    let source_scale = src_type.and_then(numeric_decimal_places).unwrap_or(0);
                    let scaled = fit_scaled_expr_to_display_target(
                        &format!("{e}.value"),
                        source_scale,
                        c_target,
                        target_size,
                        target_scale,
                        data_items,
                    );
                    let scaled = if display_numeric_target_is_unsigned(c_target, data_items) {
                        format!("llabs({scaled})")
                    } else {
                        scaled
                    };
                    emit_store_display_numeric(
                        out,
                        pad,
                        &scaled,
                        c_target,
                        target_size,
                        data_items,
                    );
                } else {
                    // CobolDecimal variable -> integer target: use the logical integer value.
                    let dec_expr = format!("cobol_decimal_to_int64(&{e})");
                    emit_store_int(out, c_target, &dec_expr, data_items, pad);
                }
            } else {
                // Use emit_int_compatible_expr to handle compound expressions
                // that may contain CobolDecimal sub-expressions.
                let e = emit_int_compatible_expr(from, data_items);
                if let Some((target_size, target_scale)) =
                    display_numeric_c_expr_info(c_target, data_items)
                {
                    let source_scale = src_type.and_then(numeric_decimal_places).unwrap_or(0);
                    let scaled = fit_scaled_expr_to_display_target(
                        &e,
                        source_scale,
                        c_target,
                        target_size,
                        target_scale,
                        data_items,
                    );
                    let scaled = if display_numeric_target_is_unsigned(c_target, data_items) {
                        format!("llabs({scaled})")
                    } else {
                        scaled
                    };
                    emit_store_display_numeric(
                        out,
                        pad,
                        &scaled,
                        c_target,
                        target_size,
                        data_items,
                    );
                } else {
                    emit_store_int(out, c_target, &e, data_items, pad);
                }
            }
        }
    }
}

/// Emit a MOVE into a reference-modified target: `MOVE src TO VAR(start:length)`.
///
/// Generated C uses `memcpy` with pointer arithmetic.
pub(crate) fn emit_move_to_refmod(
    out: &mut String,
    from: &HirExpr,
    variable: &HirDataName,
    start: &HirExpr,
    length: &Option<HirExpr>,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let c_var = data_name_to_c_name(variable);
    let c_start = emit_expr(start);
    let var_size = find_data_item_layout(&c_var, data_items).item_len;
    let c_len = if let Some(len) = length {
        emit_expr(len)
    } else {
        // No length: remaining bytes from start
        format!("({var_size} - ({c_start} - 1))")
    };

    match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let src_len = s.len();
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), \"{escaped}\", \
                 ({src_len} < (size_t)({c_len}) ? {src_len} : (size_t)({c_len})));\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Space) => {
            out.push_str(&format!(
                "{pad}memset({c_var} + ({c_start} - 1), ' ', {c_len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            out.push_str(&format!(
                "{pad}memset({c_var} + ({c_start} - 1), '0', {c_len});\n"
            ));
        }
        HirExpr::Variable(src_name) => {
            let c_src = data_name_to_c_name(src_name);
            let src_size = find_data_item_layout(&c_src, data_items).item_len;
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), {c_src}, \
                 ({src_size} < (uint32_t)({c_len}) ? {src_size} : (uint32_t)({c_len})));\n"
            ));
        }
        HirExpr::ReferenceModification {
            variable: src_var,
            start: src_start,
            length: src_length,
        } => {
            let c_src_var = data_name_to_c_name(src_var);
            let c_src_start = emit_expr(src_start);
            let src_size = find_data_item_size(&c_src_var, data_items);
            let c_src_len = if let Some(sl) = src_length {
                emit_expr(sl)
            } else {
                format!("({src_size} - ({c_src_start} - 1))")
            };
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), \
                 {c_src_var} + ({c_src_start} - 1), \
                 ({c_src_len} < ({c_len}) ? ({c_src_len}) : ({c_len})));\n"
            ));
        }
        _ => {
            // Fallback: evaluate expression, store temporarily, then memcpy
            let e = emit_expr(from);
            out.push_str(&format!(
                "{pad}{{ int64_t _tmp = {e}; \
                 memcpy({c_var} + ({c_start} - 1), &_tmp, \
                 (sizeof(_tmp) < (size_t)({c_len}) ? sizeof(_tmp) : (size_t)({c_len}))); }}\n"
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_perform(
    out: &mut String,
    kind: &HirPerformKind,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    current_paragraph: Option<HirParagraphId>,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    match kind {
        HirPerformKind::Inline { body } => {
            out.push_str(&format!("{pad}{{\n"));
            for s in body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirPerformKind::Times { count, body } => {
            let c_count = emit_int_compatible_expr(count, data_items);
            let times_id = with_active_context(|ctx| ctx.next_perform_thru_id());
            out.push_str(&format!(
                "{pad}int64_t _cobol_times_{times_id} = ({c_count});\n"
            ));
            out.push_str(&format!(
                "{pad}for (int64_t _cobol_i_{times_id} = 0; _cobol_i_{times_id} < _cobol_times_{times_id}; _cobol_i_{times_id}++) {{\n"
            ));
            for s in body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirPerformKind::Until {
            test,
            condition,
            body,
        } => {
            let cond = emit_condition(condition, data_items);
            match test {
                HirPerformTest::Before => {
                    out.push_str(&format!("{pad}while (!({cond})) {{\n"));
                    for s in body {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 1,
                        );
                    }
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirPerformTest::After => {
                    out.push_str(&format!("{pad}for (;;) {{\n"));
                    for s in body {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 1,
                        );
                    }
                    out.push_str(&format!("{pad}    if ({cond}) break;\n"));
                    out.push_str(&format!("{pad}}}\n"));
                }
            }
        }
        HirPerformKind::Varying {
            test,
            var,
            var_expr,
            from,
            by,
            until,
            after_clauses,
            body,
        } => {
            let c_var_target = varying_target_c_expr(var, var_expr, until);
            let var_is_decimal =
                find_data_item(var, data_items).is_some_and(|i| needs_decimal(&i.data_type));
            let cond = emit_condition(until, data_items);
            if var_is_decimal {
                // Decimal VARYING: use cobol_decimal operations
                emit_assign_to_decimal(out, from, &c_var_target, data_items, &pad);
                let loop_cond =
                    fast_decimal_varying_condition(var, &c_var_target, until, data_items)
                        .unwrap_or(cond);
                match test {
                    HirPerformTest::Before => {
                        out.push_str(&format!("{pad}while (!({loop_cond})) {{\n"));
                        for s in body {
                            emit_statement(
                                out,
                                s,
                                data_items,
                                paragraphs,
                                fs_map,
                                has_declaratives,
                                indent + 1,
                            );
                        }
                        let inner_pad = format!("{pad}    ");
                        if !emit_fast_decimal_varying_increment(
                            out,
                            var,
                            &c_var_target,
                            by,
                            data_items,
                            &inner_pad,
                        ) {
                            emit_decimal_arith(
                                out,
                                &c_var_target,
                                by,
                                "cobol_decimal_add",
                                data_items,
                                &inner_pad,
                            );
                        }
                        out.push_str(&format!("{pad}}}\n"));
                    }
                    HirPerformTest::After => {
                        out.push_str(&format!("{pad}for (;;) {{\n"));
                        for s in body {
                            emit_statement(
                                out,
                                s,
                                data_items,
                                paragraphs,
                                fs_map,
                                has_declaratives,
                                indent + 1,
                            );
                        }
                        out.push_str(&format!("{pad}    if ({loop_cond}) break;\n"));
                        let inner_pad = format!("{pad}    ");
                        if !emit_fast_decimal_varying_increment(
                            out,
                            var,
                            &c_var_target,
                            by,
                            data_items,
                            &inner_pad,
                        ) {
                            emit_decimal_arith(
                                out,
                                &c_var_target,
                                by,
                                "cobol_decimal_add",
                                data_items,
                                &inner_pad,
                            );
                        }
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            } else {
                let c_from = emit_int_compatible_expr(from, data_items);
                let c_by = emit_int_compatible_expr(by, data_items);
                let debug_var_name = var.as_str();
                let debug_var_width = find_data_item(var, data_items)
                    .and_then(|item| match item.data_type {
                        HirType::Numeric { size, .. } => Some(size),
                        _ => None,
                    })
                    .unwrap_or(0);
                // Initialize outer VARYING variable
                emit_store_int(out, &c_var_target, &c_from, data_items, &pad);
                emit_debug_identifier_value_event(
                    out,
                    &pad,
                    debug_var_name,
                    &c_var_target,
                    debug_var_width,
                    data_items,
                    true,
                );
                for ac in after_clauses {
                    let ac_var = varying_target_c_expr(&ac.var, &ac.var_expr, &ac.until);
                    let ac_from = emit_int_compatible_expr(&ac.from, data_items);
                    emit_store_int(out, &ac_var, &ac_from, data_items, &pad);
                    let ac_debug_var_name = ac.var.as_str();
                    let ac_debug_var_width = find_data_item(&ac.var, data_items)
                        .and_then(|item| match item.data_type {
                            HirType::Numeric { size, .. } => Some(size),
                            _ => None,
                        })
                        .unwrap_or(0);
                    emit_debug_identifier_value_event(
                        out,
                        &pad,
                        ac_debug_var_name,
                        &ac_var,
                        ac_debug_var_width,
                        data_items,
                        true,
                    );
                }
                out.push_str(&format!("{pad}for (;;) {{\n"));
                let after_indent = indent + 1;
                let after_pad = "    ".repeat(after_indent);
                let until_mentions_debug_var = condition_mentions_var(until, var);
                if matches!(test, HirPerformTest::Before) {
                    if until_mentions_debug_var {
                        emit_debug_identifier_value_event(
                            out,
                            &after_pad,
                            debug_var_name,
                            &c_var_target,
                            debug_var_width,
                            data_items,
                            true,
                        );
                    }
                    out.push_str(&format!("{after_pad}if ({cond}) break;\n"));
                }
                let mut current_indent = after_indent;
                for ac in after_clauses {
                    let ac_cond = emit_condition(&ac.until, data_items);
                    let lpad = "    ".repeat(current_indent);
                    match test {
                        HirPerformTest::Before => {
                            out.push_str(&format!("{lpad}while (!({ac_cond})) {{\n"));
                        }
                        HirPerformTest::After => {
                            out.push_str(&format!("{lpad}for (;;) {{\n"));
                        }
                    }
                    current_indent += 1;
                }
                for s in body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        current_indent,
                    );
                }
                let mut deferred_after_resets: Vec<(String, String, String, u32)> = Vec::new();
                for ac in after_clauses.iter().rev() {
                    current_indent -= 1;
                    let ac_var = varying_target_c_expr(&ac.var, &ac.var_expr, &ac.until);
                    let ac_from = emit_int_compatible_expr(&ac.from, data_items);
                    let ac_by = emit_int_compatible_expr(&ac.by, data_items);
                    let ac_cond = emit_condition(&ac.until, data_items);
                    let lpad = "    ".repeat(current_indent + 1);
                    if matches!(test, HirPerformTest::After) {
                        out.push_str(&format!("{lpad}if ({ac_cond}) break;\n"));
                    }
                    emit_store_int_op(out, &ac_var, "+", &ac_by, data_items, &lpad);
                    let ac_debug_var_name = ac.var.as_str();
                    let ac_debug_var_width = find_data_item(&ac.var, data_items)
                        .and_then(|item| match item.data_type {
                            HirType::Numeric { size, .. } => Some(size),
                            _ => None,
                        })
                        .unwrap_or(0);
                    emit_debug_identifier_value_event(
                        out,
                        &lpad,
                        ac_debug_var_name,
                        &ac_var,
                        ac_debug_var_width,
                        data_items,
                        true,
                    );
                    for (reset_var, reset_from, reset_debug_name, reset_debug_width) in
                        &deferred_after_resets
                    {
                        emit_store_int(out, reset_var, reset_from, data_items, &lpad);
                        emit_debug_identifier_value_event(
                            out,
                            &lpad,
                            reset_debug_name,
                            reset_var,
                            *reset_debug_width,
                            data_items,
                            true,
                        );
                    }
                    let lpad_close = "    ".repeat(current_indent);
                    out.push_str(&format!("{lpad_close}}}\n"));
                    deferred_after_resets.insert(
                        0,
                        (
                            ac_var,
                            ac_from,
                            ac_debug_var_name.to_string(),
                            ac_debug_var_width,
                        ),
                    );
                }
                if matches!(test, HirPerformTest::After) {
                    if until_mentions_debug_var {
                        emit_debug_identifier_value_event(
                            out,
                            &after_pad,
                            debug_var_name,
                            &c_var_target,
                            debug_var_width,
                            data_items,
                            true,
                        );
                    }
                    out.push_str(&format!("{after_pad}if ({cond}) break;\n"));
                }
                emit_store_int_op(out, &c_var_target, "+", &c_by, data_items, &after_pad);
                emit_debug_identifier_value_event(
                    out,
                    &after_pad,
                    debug_var_name,
                    &c_var_target,
                    debug_var_width,
                    data_items,
                    true,
                );
                for (reset_var, reset_from, reset_debug_name, reset_debug_width) in
                    &deferred_after_resets
                {
                    emit_store_int(out, reset_var, reset_from, data_items, &after_pad);
                    emit_debug_identifier_value_event(
                        out,
                        &after_pad,
                        reset_debug_name,
                        reset_var,
                        *reset_debug_width,
                        data_items,
                        true,
                    );
                }
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirPerformKind::ProcedureName { target, through } => {
            let c_name = transfer_target_c_name(target, paragraphs);
            let in_body = with_active_context(|ctx| ctx.in_body_context());
            let has_local_labels = with_active_context(|ctx| ctx.has_labels());
            let need_body_dispatch = in_body && has_local_labels;
            if let Some(thru) = through {
                // PERFORM name THRU through: call all paragraphs from name to through
                let c_thru = transfer_target_c_name(thru, paragraphs);
                out.push_str(&format!("{pad}/* PERFORM {c_name} THRU {c_thru} */\n"));
                let start_idx = target
                    .paragraph_id()
                    .and_then(|target_id| paragraphs.iter().position(|p| p.id == target_id));
                let end_idx = thru
                    .paragraph_id()
                    .and_then(|target_id| paragraphs.iter().position(|p| p.id == target_id));
                if let (Some(si), Some(ei)) = (start_idx, end_idx) {
                    let perform_segment = paragraphs[si].segment_number;
                    let reversed = si > ei;
                    let include_section_headers =
                        matches!(paragraphs[si].kind, HirParagraphKind::Section)
                            || matches!(paragraphs[ei].kind, HirParagraphKind::Section)
                            || (should_emit_debug_events()
                                && paragraphs[si.min(ei)..=si.max(ei)].iter().any(|paragraph| {
                                    matches!(paragraph.kind, HirParagraphKind::Section)
                                }));
                    let thru_paras: Vec<_> = if reversed {
                        let group_end = paragraph_group_end(paragraphs, si);
                        effective_perform_thru_paragraphs(
                            &paragraphs[si..group_end],
                            include_section_headers,
                        )
                    } else {
                        effective_perform_thru_paragraphs(
                            &paragraphs[si..=ei],
                            include_section_headers,
                        )
                    };
                    let non_reversed_group_end =
                        if !reversed && matches!(paragraphs[ei].kind, HirParagraphKind::Section) {
                            paragraph_group_end(paragraphs, ei)
                        } else {
                            ei + 1
                        };
                    let mut thru_ids: Vec<(HirParagraphId, usize)> = with_active_context(|ctx| {
                        thru_paras
                            .iter()
                            .flat_map(|paragraph| {
                                let mut ids = Vec::new();
                                if let Some(id) = ctx.label_id(paragraph.id) {
                                    ids.push((paragraph.id, id));
                                }
                                if let Some(id) = ctx.body_label_id(paragraph.id) {
                                    if !ids.iter().any(|(_, existing)| *existing == id) {
                                        ids.push((paragraph.id, id));
                                    }
                                }
                                ids
                            })
                            .collect()
                    });
                    let mut thru_preserve_target_ids = HashMap::new();
                    if !reversed {
                        for section_idx in si..=ei {
                            let section = &paragraphs[section_idx];
                            if !matches!(section.kind, HirParagraphKind::Section) {
                                continue;
                            }
                            if section_idx >= ei {
                                continue;
                            }
                            let Some(entry) =
                                paragraphs[section_idx + 1..=ei].iter().find(|paragraph| {
                                    thru_paras
                                        .iter()
                                        .any(|thru_paragraph| thru_paragraph.id == paragraph.id)
                                })
                            else {
                                continue;
                            };
                            with_active_context(|ctx| {
                                if let Some(id) = ctx.label_id(section.id) {
                                    if !thru_ids.iter().any(|(_, existing)| *existing == id) {
                                        thru_ids.push((entry.id, id));
                                    }
                                }
                                if let Some(id) = ctx.body_label_id(section.id) {
                                    if !thru_ids.iter().any(|(_, existing)| *existing == id) {
                                        thru_ids.push((entry.id, id));
                                    }
                                }
                            });
                        }
                        if include_section_headers {
                            let selected_section_ids: HashSet<_> = thru_paras
                                .iter()
                                .filter(|paragraph| {
                                    matches!(paragraph.kind, HirParagraphKind::Section)
                                })
                                .map(|paragraph| paragraph.id)
                                .collect();
                            for paragraph in &paragraphs[si..non_reversed_group_end] {
                                let Some(section_id) = paragraph.section_id else {
                                    continue;
                                };
                                if !selected_section_ids.contains(&section_id) {
                                    continue;
                                }
                                with_active_context(|ctx| {
                                    let local_id = local_section_label_id(paragraphs, paragraph.id);
                                    let mut add_id = |id| {
                                        if !thru_ids.iter().any(|(_, existing)| *existing == id) {
                                            thru_ids.push((section_id, id));
                                        }
                                        thru_preserve_target_ids.insert(id, local_id.unwrap_or(id));
                                    };
                                    if let Some(id) = ctx.label_id(paragraph.id) {
                                        add_id(id);
                                    }
                                    if let Some(id) = ctx.body_label_id(paragraph.id) {
                                        add_id(id);
                                    }
                                });
                            }
                        }
                    }
                    let dispatch_ids: Vec<(HirParagraphId, usize)> = with_active_context(|ctx| {
                        paragraphs
                            .iter()
                            .flat_map(|paragraph| {
                                let mut ids = Vec::new();
                                if let Some(id) = ctx.label_id(paragraph.id) {
                                    ids.push((paragraph.id, id));
                                }
                                if let Some(id) = ctx.body_label_id(paragraph.id) {
                                    if !ids.iter().any(|(_, existing)| *existing == id) {
                                        ids.push((paragraph.id, id));
                                    }
                                }
                                ids
                            })
                            .collect()
                    });
                    let fallthrough_ids: HashMap<usize, usize> = with_active_context(|ctx| {
                        let mut ids = HashMap::new();
                        for pair in paragraphs.windows(2) {
                            let current = &pair[0];
                            let next = &pair[1];
                            let Some(next_id) = ctx.label_id(next.id) else {
                                continue;
                            };
                            if let Some(id) = ctx.label_id(current.id) {
                                ids.insert(id, next_id);
                            }
                            if let Some(id) = ctx.body_label_id(current.id) {
                                ids.insert(id, next_id);
                            }
                        }
                        ids
                    });
                    let after_thru_ids: Vec<usize> = if !reversed {
                        paragraphs
                            .get(non_reversed_group_end)
                            .map(|paragraph| {
                                with_active_context(|ctx| {
                                    let mut ids = Vec::new();
                                    if let Some(id) = ctx.label_id(paragraph.id) {
                                        ids.push(id);
                                    }
                                    if let Some(id) = ctx.body_label_id(paragraph.id) {
                                        if !ids.contains(&id) {
                                            ids.push(id);
                                        }
                                    }
                                    ids
                                })
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let enclosing_section_ids: Vec<usize> = if !reversed {
                        paragraphs[..=si]
                            .iter()
                            .rposition(|paragraph| {
                                matches!(paragraph.kind, HirParagraphKind::Section)
                            })
                            .map(|section_idx| {
                                let section = &paragraphs[section_idx];
                                with_active_context(|ctx| {
                                    let mut ids = Vec::new();
                                    if let Some(id) = ctx.label_id(section.id) {
                                        ids.push(id);
                                    }
                                    if let Some(id) = ctx.body_label_id(section.id) {
                                        if !ids.contains(&id) {
                                            ids.push(id);
                                        }
                                    }
                                    ids
                                })
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let reversed_thru_ids: Vec<usize> = if reversed {
                        thru.paragraph_id()
                            .map(|through_id| {
                                dispatch_ids
                                    .iter()
                                    .filter_map(|(paragraph_id, label_id)| {
                                        (*paragraph_id == through_id).then_some(*label_id)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    if !thru_ids.is_empty() && (thru_paras.len() > 1 || reversed) {
                        // Generate unique label suffix for this PERFORM THRU
                        let pt_id = with_active_context(|ctx| ctx.next_perform_thru_id());
                        let suffix = format!("pt{pt_id}");
                        let suppress_thru_segment = should_suppress_segment_reset(
                            paragraphs[si].id,
                            paragraphs,
                            current_paragraph,
                        );

                        if suppress_thru_segment {
                            out.push_str(&format!("{pad}_suppress_segment_reset = 1;\n"));
                        }
                        out.push_str(&format!(
                            "{pad}int _suppress_segment_resume_{suffix} = 0;\n"
                        ));
                        out.push_str(&format!(
                            "{pad}int _suppress_segment_preserved_{suffix} = 0;\n"
                        ));
                        out.push_str(&format!("{pad}int _perform_external_flow_{suffix} = 0;\n"));

                        let through_section_id = thru
                            .paragraph_id()
                            .and_then(|through_id| {
                                paragraphs
                                    .iter()
                                    .find(|paragraph| paragraph.id == through_id)
                            })
                            .and_then(paragraph_section_id);

                        // Emit each paragraph call with goto dispatch
                        for (idx, paragraph) in thru_paras.iter().enumerate() {
                            let pn = sanitize_name(&paragraph.name);
                            let debug_name = escape_c_string(&paragraph.name);
                            let debug_contents = if idx == 0 {
                                "PERFORM LOOP"
                            } else {
                                "FALL THROUGH"
                            };
                            out.push_str(&format!("_pt_{suffix}_{pn}:\n"));
                            emit_optional_debug_event(out, &pad, &debug_name, debug_contents);
                            out.push_str(&format!(
                                "{pad}if (_suppress_segment_resume_{suffix}) {{ _suppress_segment_reset = 1; }}\n"
                            ));
                            if !suppress_thru_segment {
                                emit_segment_reset_suppression_start(
                                    out,
                                    paragraph.id,
                                    paragraphs,
                                    current_paragraph,
                                    &pad,
                                );
                            }
                            emit_independent_segment_state_save(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                                &format!("{suffix}_{idx}"),
                            );
                            let section_entry_id = paragraph.section_id.filter(|section_id| {
                                reversed && through_section_id != Some(*section_id)
                            });
                            if let (Some(section_id), Some(local_id)) = (
                                section_entry_id,
                                local_section_label_id(paragraphs, paragraph.id),
                            ) {
                                if let Some(section_pn) = paragraph_c_name(paragraphs, section_id) {
                                    out.push_str(&format!(
                                        "{pad}_goto_target = {local_id}; para_{section_pn}();\n"
                                    ));
                                } else {
                                    out.push_str(&format!("{pad}para_{pn}();\n"));
                                }
                            } else {
                                out.push_str(&format!("{pad}para_{pn}();\n"));
                            }
                            emit_independent_segment_state_restore(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                                &format!("{suffix}_{idx}"),
                            );
                            if !suppress_thru_segment {
                                emit_segment_reset_suppression_end(
                                    out,
                                    paragraph.id,
                                    paragraphs,
                                    current_paragraph,
                                    &pad,
                                );
                            }
                            if suppress_thru_segment {
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix} && _suppress_segment_preserved_{suffix} && _goto_target == _suppress_segment_preserved_{suffix}) {{ _goto_target = 0; }}\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix}) {{ _suppress_segment_preserved_{suffix} = 0; }}\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix}) {{ _suppress_segment_resume_{suffix} = 0; }}\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix} && _suppress_segment_preserved_{suffix} && _goto_target == _suppress_segment_preserved_{suffix}) {{ _goto_target = 0; }}\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix}) {{ _suppress_segment_preserved_{suffix} = 0; }}\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}if (_suppress_segment_resume_{suffix}) {{ _suppress_segment_reset = 0; _suppress_segment_resume_{suffix} = 0; }}\n"
                                ));
                            }
                            if idx < thru_paras.len() - 1 {
                                // After each call (except last), check _goto_target
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                ));
                            } else {
                                // After last call, check for out-of-range goto
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                ));
                            }
                        }
                        out.push_str(&format!("{pad}goto _pt_end_{suffix};\n"));

                        // Dispatch table for this PERFORM THRU
                        out.push_str(&format!("_pt_disp_{suffix}:\n"));
                        out.push_str(&format!("{pad}{{ int _t = _goto_target;\n"));
                        if reversed {
                            let thru_pn = transfer_target_c_name(thru, paragraphs);
                            for id in &reversed_thru_ids {
                                out.push_str(&format!(
                                    "{pad}  if (_t == {id}) {{ _goto_target = 0; para_{thru_pn}(); _goto_target = 0; goto _pt_end_{suffix}; }}\n"
                                ));
                            }
                        }
                        for (paragraph_id, id) in &dispatch_ids {
                            if reversed_thru_ids.contains(id) {
                                continue;
                            }
                            let Some(pn) = paragraph_c_name(paragraphs, *paragraph_id) else {
                                continue;
                            };
                            if reversed {
                                out.push_str(&format!(
                                    "{pad}  if (_t == {id}) {{ _goto_target = 0; "
                                ));
                                emit_perform_segment_dispatch_call(
                                    out,
                                    *paragraph_id,
                                    &pn,
                                    paragraphs,
                                    perform_segment,
                                    current_paragraph,
                                    &pad,
                                    &suffix,
                                    suppress_thru_segment,
                                );
                                out.push_str(&format!(
                                    "_goto_target = 0; goto _pt_end_{suffix}; }}\n"
                                ));
                            } else if let Some((entry_paragraph_id, _)) = thru_ids
                                .iter()
                                .find(|(_, thru_label_id)| thru_label_id == id)
                            {
                                let Some(entry_pn) =
                                    paragraph_c_name(paragraphs, *entry_paragraph_id)
                                else {
                                    continue;
                                };
                                if let Some(local_id) = thru_preserve_target_ids.get(id) {
                                    out.push_str(&format!(
                                        "{pad}  if (_t == {id}) {{ _perform_external_flow_{suffix} = 0; _goto_target = {local_id}; _suppress_segment_preserved_{suffix} = {local_id}; _suppress_segment_resume_{suffix} = 1; goto _pt_{suffix}_{entry_pn}; }}\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "{pad}  if (_t == {id}) {{ _perform_external_flow_{suffix} = 0; _goto_target = 0; _suppress_segment_resume_{suffix} = 1; goto _pt_{suffix}_{entry_pn}; }}\n"
                                    ));
                                }
                            } else {
                                out.push_str(&format!("{pad}  if (_t == {id}) {{ "));
                                if is_same_procedure_scope(
                                    *paragraph_id,
                                    paragraphs,
                                    current_paragraph,
                                ) {
                                    out.push_str(&format!(
                                        "_perform_external_flow_{suffix} = 1; _goto_target = 0; "
                                    ));
                                    if suppress_thru_segment {
                                        out.push_str("_suppress_segment_reset = 1; ");
                                    }
                                    out.push_str(&format!("para_{pn}(); "));
                                    if suppress_thru_segment {
                                        out.push_str("_suppress_segment_reset = 0; ");
                                    }
                                    if let Some(next_id) = fallthrough_ids.get(id) {
                                        out.push_str(&format!(
                                            "if (!_goto_target) {{ _goto_target = {next_id}; }} "
                                        ));
                                    }
                                    out.push_str(&format!(
                                        "if (_goto_target) goto _pt_disp_{suffix}; "
                                    ));
                                    if suppress_thru_segment {
                                        out.push_str("_suppress_segment_reset = 0; ");
                                    }
                                    out.push_str(&format!("goto _pt_end_{suffix}; }}\n"));
                                } else {
                                    out.push_str(&format!(
                                        "if (_perform_external_flow_{suffix}) {{ _goto_target = _t; "
                                    ));
                                    if suppress_thru_segment {
                                        out.push_str("_suppress_segment_reset = 0; ");
                                    }
                                    out.push_str("goto _goto_dispatch; } ");
                                    out.push_str("_goto_target = 0; ");
                                    if suppress_thru_segment {
                                        out.push_str("_suppress_segment_reset = 0; ");
                                    }
                                    out.push_str(&format!("goto _pt_end_{suffix}; }}\n"));
                                }
                            }
                        }
                        if !after_thru_ids.is_empty() {
                            for id in &after_thru_ids {
                                out.push_str(&format!(
                                    "{pad}  if (_t == {id}) {{ if (_perform_external_flow_{suffix}) {{ _goto_target = _t; goto _goto_dispatch; }} _goto_target = 0; goto _pt_end_{suffix}; }}\n"
                                ));
                            }
                        }
                        if !enclosing_section_ids.is_empty() {
                            for id in &enclosing_section_ids {
                                out.push_str(&format!(
                                    "{pad}  if (_t == {id}) {{ if (_perform_external_flow_{suffix}) {{ _goto_target = _t; goto _goto_dispatch; }} _goto_target = 0; goto _pt_end_{suffix}; }}\n"
                                ));
                            }
                        }
                        // Not in range: propagate
                        if suppress_thru_segment {
                            out.push_str(&format!("{pad}  _suppress_segment_reset = 0;\n"));
                        }
                        out.push_str(&format!("{pad}  goto _goto_dispatch;\n"));
                        out.push_str(&format!("{pad}}}\n"));
                        out.push_str(&format!("_pt_end_{suffix}:;\n"));
                        if suppress_thru_segment {
                            out.push_str(&format!("{pad}_suppress_segment_reset = 0;\n"));
                        }
                    } else {
                        for paragraph in &thru_paras {
                            let pn = sanitize_name(&paragraph.name);
                            let debug_name = escape_c_string(&paragraph.name);
                            let debug_contents = if pn == c_name {
                                "PERFORM LOOP"
                            } else {
                                "FALL THROUGH"
                            };
                            emit_optional_debug_event(out, &pad, &debug_name, debug_contents);
                            emit_segment_reset_suppression_start(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                            );
                            emit_independent_segment_state_save(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                                &format!("seq_{}", paragraph.id.0),
                            );
                            out.push_str(&format!("{pad}para_{pn}();\n"));
                            emit_independent_segment_state_restore(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                                &format!("seq_{}", paragraph.id.0),
                            );
                            emit_segment_reset_suppression_end(
                                out,
                                paragraph.id,
                                paragraphs,
                                current_paragraph,
                                &pad,
                            );
                            if need_body_dispatch {
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _goto_dispatch;\n"
                                ));
                            } else if has_local_labels {
                                out.push_str(&format!("{pad}_goto_target = 0;\n"));
                            }
                        }
                    }
                } else {
                    // Fallback: just call the named paragraph
                    let debug_name = escape_c_string(target.name());
                    emit_optional_debug_event(out, &pad, &debug_name, "PERFORM LOOP");
                    if let Some(target_id) = target.paragraph_id() {
                        emit_segment_reset_suppression_start(
                            out,
                            target_id,
                            paragraphs,
                            current_paragraph,
                            &pad,
                        );
                        emit_independent_segment_state_save(
                            out,
                            target_id,
                            paragraphs,
                            current_paragraph,
                            &pad,
                            "single",
                        );
                    }
                    out.push_str(&format!("{pad}para_{c_name}();\n"));
                    if let Some(target_id) = target.paragraph_id() {
                        emit_independent_segment_state_restore(
                            out,
                            target_id,
                            paragraphs,
                            current_paragraph,
                            &pad,
                            "single",
                        );
                        emit_segment_reset_suppression_end(
                            out,
                            target_id,
                            paragraphs,
                            current_paragraph,
                            &pad,
                        );
                    }
                    if need_body_dispatch {
                        out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                    } else if has_local_labels {
                        out.push_str(&format!("{pad}_goto_target = 0;\n"));
                    }
                }
            } else {
                let debug_name = escape_c_string(target.name());
                emit_optional_debug_event(out, &pad, &debug_name, "PERFORM LOOP");
                if let Some(target_id) = target.paragraph_id() {
                    emit_segment_reset_suppression_start(
                        out,
                        target_id,
                        paragraphs,
                        current_paragraph,
                        &pad,
                    );
                    emit_independent_segment_state_save(
                        out,
                        target_id,
                        paragraphs,
                        current_paragraph,
                        &pad,
                        "single",
                    );
                }
                out.push_str(&format!("{pad}para_{c_name}();\n"));
                if let Some(target_id) = target.paragraph_id() {
                    emit_independent_segment_state_restore(
                        out,
                        target_id,
                        paragraphs,
                        current_paragraph,
                        &pad,
                        "single",
                    );
                    emit_segment_reset_suppression_end(
                        out,
                        target_id,
                        paragraphs,
                        current_paragraph,
                        &pad,
                    );
                }
                if need_body_dispatch {
                    out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                } else if has_local_labels {
                    // Direct PERFORM in a paragraph/section function is
                    // already expanded at the call site, so nested control
                    // transfers must not leak into the caller's dispatch loop.
                    out.push_str(&format!("{pad}_goto_target = 0;\n"));
                }
            }
        }
    }
}

fn emit_segment_reset_suppression_start(
    out: &mut String,
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
    pad: &str,
) {
    if should_suppress_segment_reset(target_id, paragraphs, current_paragraph) {
        out.push_str(&format!("{pad}_suppress_segment_reset = 1;\n"));
    }
}

fn emit_segment_reset_suppression_end(
    out: &mut String,
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
    pad: &str,
) {
    if should_suppress_segment_reset(target_id, paragraphs, current_paragraph) {
        out.push_str(&format!("{pad}_suppress_segment_reset = 0;\n"));
    }
}

fn emit_independent_segment_state_save(
    out: &mut String,
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
    pad: &str,
    suffix: &str,
) {
    let vars = current_independent_section_alter_vars(target_id, paragraphs, current_paragraph);
    if vars.is_empty() {
        return;
    }
    out.push_str(&format!("{pad}{{\n"));
    for (idx, var) in vars.iter().enumerate() {
        out.push_str(&format!(
            "{pad}    uint32_t _saved_alter_{suffix}_{idx} = {var};\n"
        ));
    }
}

fn emit_independent_segment_state_restore(
    out: &mut String,
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
    pad: &str,
    suffix: &str,
) {
    let vars = current_independent_section_alter_vars(target_id, paragraphs, current_paragraph);
    for (idx, var) in vars.iter().enumerate() {
        out.push_str(&format!("{pad}    {var} = _saved_alter_{suffix}_{idx};\n"));
    }
    if !vars.is_empty() {
        out.push_str(&format!("{pad}}}\n"));
    }
}

fn current_independent_section_alter_vars(
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
) -> Vec<String> {
    let Some(current_id) = current_paragraph else {
        return Vec::new();
    };
    let Some(target) = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == target_id)
    else {
        return Vec::new();
    };
    let Some(current) = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == current_id)
    else {
        return Vec::new();
    };
    if current
        .segment_number
        .is_none_or(|segment_number| segment_number <= 49)
        || target
            .segment_number
            .is_none_or(|segment_number| segment_number <= 49)
    {
        return Vec::new();
    }
    let current_section = paragraph_section_id(current);
    let target_section = paragraph_section_id(target);
    if current_section.is_none() || current_section == target_section {
        return Vec::new();
    }

    let Some(current_section) = current_section else {
        return Vec::new();
    };
    with_active_context(|ctx| {
        paragraphs
            .iter()
            .filter(|paragraph| paragraph_section_id(paragraph) == Some(current_section))
            .filter_map(|paragraph| {
                ctx.alterable_paragraph(paragraph.id)
                    .map(|info| info.dispatch_var)
            })
            .collect()
    })
}

fn paragraph_section_id(paragraph: &HirParagraph) -> Option<HirParagraphId> {
    if matches!(paragraph.kind, HirParagraphKind::Section) {
        Some(paragraph.id)
    } else {
        paragraph.section_id
    }
}

fn local_section_label_id(
    paragraphs: &[HirParagraph],
    paragraph_id: HirParagraphId,
) -> Option<usize> {
    let paragraph = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == paragraph_id)?;
    let section_id = paragraph.section_id?;
    let mut local_id = 1usize;
    for candidate in paragraphs {
        if candidate.id == section_id {
            continue;
        }
        if candidate.section_id == Some(section_id) {
            local_id += 1;
            if candidate.id == paragraph_id {
                return Some(local_id);
            }
        }
    }
    None
}

fn should_suppress_segment_reset(
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
) -> bool {
    let Some(current_id) = current_paragraph else {
        return false;
    };
    let target_segment = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == target_id)
        .and_then(|paragraph| paragraph.segment_number);
    if target_segment.is_none_or(|number| number <= 49) {
        return false;
    }
    let current_segment = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == current_id)
        .and_then(|paragraph| paragraph.segment_number);
    current_segment == target_segment
}

fn is_same_procedure_scope(
    target_id: HirParagraphId,
    paragraphs: &[HirParagraph],
    current_paragraph: Option<HirParagraphId>,
) -> bool {
    let Some(current_id) = current_paragraph else {
        return true;
    };
    let Some(current) = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == current_id)
    else {
        return true;
    };
    let Some(target) = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == target_id)
    else {
        return false;
    };
    if target.id == current.id {
        return true;
    }
    let current_section = if matches!(current.kind, HirParagraphKind::Section) {
        Some(current.id)
    } else {
        current.section_id
    };
    let target_section = if matches!(target.kind, HirParagraphKind::Section) {
        Some(target.id)
    } else {
        target.section_id
    };
    current_section.is_some() && current_section == target_section
}

#[allow(clippy::too_many_arguments)]
fn emit_perform_segment_dispatch_call(
    out: &mut String,
    target_id: HirParagraphId,
    target_c_name: &str,
    paragraphs: &[HirParagraph],
    perform_segment: Option<u32>,
    current_paragraph: Option<HirParagraphId>,
    _pad: &str,
    suffix: &str,
    already_suppressed: bool,
) {
    let target_segment = paragraphs
        .iter()
        .find(|paragraph| paragraph.id == target_id)
        .and_then(|paragraph| paragraph.segment_number);
    let current_segment = current_paragraph.and_then(|current_id| {
        paragraphs
            .iter()
            .find(|paragraph| paragraph.id == current_id)
            .and_then(|paragraph| paragraph.segment_number)
    });
    let suppress = target_segment.is_some_and(|number| number > 49)
        && (target_segment == perform_segment || target_segment == current_segment);
    if already_suppressed && !suppress {
        out.push_str("_suppress_segment_reset = 0; ");
    }
    if suppress && !already_suppressed {
        out.push_str("_suppress_segment_reset = 1; ");
    }
    out.push_str(&format!("para_{target_c_name}(); "));
    if suppress && !already_suppressed {
        out.push_str("_suppress_segment_reset = 0; ");
    }
    out.push_str(&format!("if (_goto_target) goto _pt_disp_{suffix}; "));
}

fn emit_move_to_alphanumeric_edited(
    out: &mut String,
    from: &HirExpr,
    target_item: &HirDataItem,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let Some(pic) = &target_item.picture else {
        return false;
    };
    let escaped_pic = escape_c_string(pic);
    let pic_len = pic.len();
    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
    if matches!(
        from,
        HirExpr::Literal(HirLiteral::Zero)
            | HirExpr::Literal(HirLiteral::Space)
            | HirExpr::Literal(HirLiteral::HighValue)
            | HirExpr::Literal(HirLiteral::LowValue)
            | HirExpr::Literal(HirLiteral::Quote)
    ) {
        let fill = match from {
            HirExpr::Literal(HirLiteral::Zero) => "'0'",
            HirExpr::Literal(HirLiteral::Space) => "' '",
            HirExpr::Literal(HirLiteral::HighValue) => "0xFF",
            HirExpr::Literal(HirLiteral::LowValue) => "0x00",
            HirExpr::Literal(HirLiteral::Quote) => "'\"'",
            _ => unreachable!(),
        };
        out.push_str(&format!(
            "{pad}{{ uint8_t _fig[256]; memset(_fig, {fill}, {tgt_size}); \
             cobol_move_alphanumeric_edited((const uint8_t*)_fig, {tgt_size}, \
             (uint8_t*){c_target}, {tgt_size}, (const uint8_t*)\"{escaped_pic}\", {pic_len}); }}\n"
        ));
        return true;
    }
    let (src_ptr, src_len) = match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            (
                format!("(const uint8_t*)\"{escaped}\""),
                s.len().to_string(),
            )
        }
        HirExpr::Literal(HirLiteral::Integer(n)) if *n >= 0 => {
            let digits = n.to_string();
            let source = if pic.contains('0') {
                format!("0{digits}")
            } else {
                digits
            };
            let len = source.len();
            (format!("(const uint8_t*)\"{source}\""), len.to_string())
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) if d.chars().all(|ch| ch.is_ascii_digit()) => {
            let escaped = escape_c_string(d);
            (
                format!("(const uint8_t*)\"{escaped}\""),
                d.len().to_string(),
            )
        }
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            if !is_alpha_expr(from, data_items) && alphanumeric_expr_len(from, data_items).is_none()
            {
                let c_source = emit_expr(from);
                if let Some(item) = expr_data_name(from)
                    .and_then(|name| find_data_item_by_name(name, data_items))
                    .filter(|item| item.sign.is_some_and(|sign| sign.separate))
                {
                    let value = emit_int_compatible_expr(from, data_items);
                    let digit_size = match item.data_type {
                        HirType::Numeric { size, .. } => size,
                        _ => find_data_item_layout(&c_source, data_items).item_len,
                    };
                    out.push_str(&format!(
                        "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%0*lld\", {digit_size}, (long long)llabs({value})); \
                         cobol_move_alphanumeric_edited((const uint8_t*)_nbuf, (uint32_t)_nlen, \
                         (uint8_t*){c_target}, {tgt_size}, (const uint8_t*)\"{escaped_pic}\", {pic_len}); }}\n"
                    ));
                    return true;
                }
                if let Some(src_size) = grp_display_size(&c_source, data_items) {
                    out.push_str(&format!(
                        "{pad}cobol_move_alphanumeric_edited({}, {src_size}, \
                         (uint8_t*){c_target}, {tgt_size}, \
                         (const uint8_t*)\"{escaped_pic}\", {pic_len});\n",
                        display_numeric_const_ptr(&c_source)
                    ));
                    return true;
                }
                let value = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!(
                    "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%lld\", (long long)llabs({value})); \
                     cobol_move_alphanumeric_edited((const uint8_t*)_nbuf, (uint32_t)_nlen, \
                     (uint8_t*){c_target}, {tgt_size}, (const uint8_t*)\"{escaped_pic}\", {pic_len}); }}\n"
                ));
                return true;
            }
            emit_alphanumeric_operand(from, data_items)
        }
        HirExpr::ReferenceModification { .. } => emit_alphanumeric_operand(from, data_items),
        _ => return false,
    };
    out.push_str(&format!(
        "{pad}cobol_move_alphanumeric_edited({src_ptr}, {src_len}, \
         (uint8_t*){c_target}, {tgt_size}, (const uint8_t*)\"{escaped_pic}\", {pic_len});\n"
    ));
    true
}

fn emit_move_numeric_to_alphanumeric(
    out: &mut String,
    value_expr: &str,
    c_target: &str,
    target_size: u32,
    pad: &str,
) {
    out.push_str(&format!(
        "{pad}{{ char _nbuf[64]; int _nlen = snprintf(_nbuf, sizeof(_nbuf), \"%lld\", (long long)llabs({value_expr})); \
         cobol_move_string((const uint8_t*)_nbuf, (uint32_t)_nlen, (uint8_t*){c_target}, {target_size}); }}\n"
    ));
}

fn emit_move_numeric_item_to_alphanumeric(
    out: &mut String,
    from: &HirExpr,
    value_expr: &str,
    c_target: &str,
    target_size: u32,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let Some(item) = expr_data_name(from).and_then(|name| find_data_item_by_name(name, data_items))
    else {
        return false;
    };
    let Some(pic) = &item.picture else {
        return false;
    };
    let HirType::Numeric {
        size,
        decimal_places,
        is_signed,
    } = item.data_type
    else {
        return false;
    };
    let display_pic = if item.scale_adjustment > 0 {
        "9".repeat((size + item.scale_adjustment as u32) as usize)
    } else if item.scale_adjustment < 0 {
        "9".repeat(size as usize)
    } else {
        pic.to_string()
    };
    let escaped_pic = escape_c_string(&display_pic);
    let pic_len = display_pic.len();
    let signed = if is_signed { "1" } else { "0" };
    out.push_str(&format!(
        "{pad}{{ CobolDecimal _src_dec = {{ .value = ({value_expr}), .scale = {decimal_places}, \
         .size = {pic_len}, .is_signed = {signed} }}; \
         char _dbuf[256]; uint32_t _dlen = cobol_decimal_to_display(\
         &_src_dec, (uint8_t*)_dbuf, 256, (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
         cobol_move_string((const uint8_t*)_dbuf, _dlen, (uint8_t*){c_target}, {target_size}); }}\n"
    ));
    true
}

fn numeric_decimal_places(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Numeric { decimal_places, .. } | HirType::Comp3 { decimal_places, .. } => {
            Some(*decimal_places)
        }
        _ => None,
    }
}

fn fit_scaled_expr_to_display_target(
    value_expr: &str,
    source_scale: u32,
    c_target: &str,
    target_size: u32,
    target_scale: u32,
    data_items: &[HirDataItem],
) -> String {
    let target_digits = display_numeric_digit_count(c_target, target_size, data_items);
    if target_digits == 0 {
        return "0".to_string();
    }
    let target_scale_adjustment = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .map_or(0, |item| item.scale_adjustment);
    let value = if target_scale_adjustment > 0 {
        let factor = pow10_i64_literal(target_scale_adjustment as u32);
        format!("(((int64_t)({value_expr})) / {factor})")
    } else {
        format!("((int64_t)({value_expr}))")
    };
    match target_scale.cmp(&source_scale) {
        std::cmp::Ordering::Greater => {
            let scale_delta = target_scale - source_scale;
            if scale_delta >= target_digits {
                return "0".to_string();
            }
            let keep_digits = target_digits - scale_delta;
            let reduced = modulo_signed_expr(&value, keep_digits);
            let factor = pow10_i64_literal(scale_delta);
            format!("(({reduced}) * {factor})")
        }
        std::cmp::Ordering::Less => {
            let factor = pow10_i64_literal(source_scale - target_scale);
            let scaled_down = format!("(({value}) / {factor})");
            modulo_signed_expr(&scaled_down, target_digits)
        }
        std::cmp::Ordering::Equal => modulo_signed_expr(&value, target_digits),
    }
}

fn display_numeric_digit_count(
    c_target: &str,
    target_size: u32,
    data_items: &[HirDataItem],
) -> u32 {
    find_data_item_by_c_name_or_leaf(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .and_then(|item| match item.data_type {
            HirType::Numeric { size, .. } => Some(size),
            _ => None,
        })
        .unwrap_or(target_size)
}

fn modulo_signed_expr(value_expr: &str, digits: u32) -> String {
    if digits == 0 {
        return "0".to_string();
    }
    if digits > 18 {
        return value_expr.to_string();
    }
    let factor = pow10_i64_literal(digits);
    format!("(({value_expr} >= 0) ? ({value_expr} % {factor}) : -((-{value_expr}) % {factor}))")
}

fn display_numeric_target_is_unsigned(c_target: &str, data_items: &[HirDataItem]) -> bool {
    fn is_unsigned_display_item(item: &HirDataItem) -> bool {
        !item.is_numeric_edited
            && matches!(
                item.data_type,
                HirType::Numeric {
                    is_signed: false,
                    ..
                }
            )
            && item
                .picture
                .as_ref()
                .is_none_or(|pic| !pic.to_ascii_uppercase().contains('S'))
    }

    if let Some(item) = find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
    {
        if matches!(item.data_type, HirType::Numeric { .. }) {
            return is_unsigned_display_item(item);
        }
    }

    let leaf = c_target.rsplit("__").next().unwrap_or(c_target);
    let mut matches = Vec::new();
    collect_data_items_by_sanitized_name(leaf, data_items, &mut matches);
    !matches.is_empty()
        && matches.iter().all(|item| {
            !item.is_numeric_edited
                && matches!(
                    item.data_type,
                    HirType::Numeric {
                        is_signed: false,
                        ..
                    }
                )
                && item
                    .picture
                    .as_ref()
                    .is_none_or(|pic| !pic.to_ascii_uppercase().contains('S'))
        })
}

fn find_data_item_by_c_name_or_leaf<'a>(
    c_name: &str,
    data_items: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    find_data_item_by_c_name(c_name, data_items).or_else(|| {
        let leaf = c_name.rsplit("__").next().unwrap_or(c_name);
        let mut matches = Vec::new();
        collect_data_items_by_sanitized_name(leaf, data_items, &mut matches);
        if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        }
    })
}

fn collect_data_items_by_sanitized_name<'a>(
    c_name: &str,
    data_items: &'a [HirDataItem],
    matches: &mut Vec<&'a HirDataItem>,
) {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            matches.push(item);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            collect_data_items_by_sanitized_name(c_name, members, matches);
        }
    }
}

fn emit_store_display_numeric(
    out: &mut String,
    pad: &str,
    value_expr: &str,
    c_target: &str,
    target_size: u32,
    data_items: &[HirDataItem],
) {
    let target_ptr = display_numeric_ptr(c_target);
    let blank_when_zero = find_data_item_by_c_name_or_leaf(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .is_some_and(|item| item.blank_when_zero);
    if blank_when_zero {
        out.push_str(&format!(
            "{pad}if (({value_expr}) == 0) {{ memset({target_ptr}, ' ', {target_size}); }} else {{\n"
        ));
    }
    let store_pad = if blank_when_zero {
        format!("{pad}    ")
    } else {
        pad.to_string()
    };
    if let Some(sign) = display_numeric_sign_clause(c_target, data_items) {
        if sign.separate {
            let position = match sign.position {
                HirSignPosition::Leading => 0,
                HirSignPosition::Trailing => 1,
            };
            out.push_str(&format!(
                "{store_pad}cobol_store_numeric_display_separate_sign({value_expr}, {target_ptr}, {target_size}, {position});\n"
            ));
        } else if matches!(sign.position, HirSignPosition::Leading) {
            out.push_str(&format!(
                "{store_pad}cobol_store_numeric_display_leading_sign({value_expr}, {target_ptr}, {target_size});\n"
            ));
        } else {
            out.push_str(&format!(
                "{store_pad}cobol_store_numeric_display({value_expr}, {target_ptr}, {target_size});\n"
            ));
        }
    } else if display_numeric_target_is_unsigned(c_target, data_items) {
        out.push_str(&format!(
            "{store_pad}cobol_store_numeric_display(llabs((int64_t)({value_expr})), {target_ptr}, {target_size});\n"
        ));
    } else {
        out.push_str(&format!(
            "{store_pad}cobol_store_numeric_display({value_expr}, {target_ptr}, {target_size});\n"
        ));
    }
    if blank_when_zero {
        out.push_str(&format!("{pad}}}\n"));
    }
}

fn display_numeric_sign_clause(
    c_target: &str,
    data_items: &[HirDataItem],
) -> Option<cobol_hir::HirSignClause> {
    let item = find_data_item_by_c_name_or_leaf(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))?;
    item.sign
}

fn expr_name_is_display_numeric(name: &HirDataName) -> bool {
    let c_name = data_name_to_c_name(name);
    c_expr_is_display_numeric(&c_name)
}

fn c_expr_is_display_numeric(c_expr: &str) -> bool {
    with_active_context(|ctx| {
        if ctx.has_display_numeric(c_expr) {
            return true;
        }
        ctx.has_display_numeric(extract_leaf_member(c_expr))
    })
}

fn target_expr_is_decimal(target: &HirExpr, c_target: &str, data_items: &[HirDataItem]) -> bool {
    is_decimal_expr(target, data_items)
        || find_data_item_by_c_name(c_target, data_items)
            .or_else(|| find_data_item(c_target, data_items))
            .is_some_and(|item| needs_decimal(&item.data_type))
}

fn is_alphanumeric_edited_item(item: &HirDataItem) -> bool {
    if item.is_numeric_edited || !matches!(item.data_type, HirType::Alphanumeric { .. }) {
        return false;
    }
    item.picture
        .as_ref()
        .is_some_and(|pic| picture_has_alphanumeric_editing(pic))
}

fn picture_has_alphanumeric_editing(pic: &str) -> bool {
    let mut has_data_char = false;
    let mut has_edit_char = false;
    let mut in_repeat = false;
    for ch in pic.chars().map(|ch| ch.to_ascii_uppercase()) {
        match ch {
            '(' => in_repeat = true,
            ')' => in_repeat = false,
            'A' | 'X' | '9' if !in_repeat => has_data_char = true,
            'B' | '0' | '/' | ',' | '.' if !in_repeat => has_edit_char = true,
            _ => {}
        }
    }
    has_data_char && has_edit_char
}

fn emit_store_decimal_to_numeric_edited(
    out: &mut String,
    c_target: &str,
    decimal_expr: &str,
    target_item: &HirDataItem,
    rounded: bool,
    pad: &str,
) {
    let Some(pic) = &target_item.picture else {
        return;
    };
    let escaped_pic = escape_c_string(pic);
    let pic_len = pic.len();
    let tgt_size = data_item_byte_size(&target_item.data_type);
    let decimal_init = if let Some((src_size, src_scale)) =
        display_numeric_c_expr_info(decimal_expr, &[])
    {
        let src_ptr = display_numeric_const_ptr(decimal_expr);
        format!(
            "CobolDecimal _ned; cobol_decimal_from_int(cobol_display_to_int64({src_ptr}, {src_size}), {src_scale}, &_ned); "
        )
    } else {
        format!("CobolDecimal _ned = {decimal_expr}; ")
    };
    let normalize = if rounded {
        let scale = numeric_edited_picture_scale(pic);
        format!(
            "_ned.value = llround(cobol_decimal_to_double(&_ned) * pow(10.0, {scale})); _ned.scale = {scale}; "
        )
    } else {
        String::new()
    };

    out.push_str(&format!(
        "{pad}{decimal_init}{normalize}\
         char _ned_buf[256]; uint32_t _ned_len = cobol_decimal_to_display(\
         &_ned, (uint8_t*)_ned_buf, 256, (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
         cobol_move_string((const uint8_t*)_ned_buf, _ned_len, (uint8_t*){c_target}, {tgt_size}); "
    ));
}

fn emit_size_checked_int_assignment(
    out: &mut String,
    c_target: &str,
    result_expr: &str,
    target_name: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some((disp_size, _scale, _signed)) =
        display_numeric_c_expr_metadata(c_target, data_items)
            .or_else(|| grp_display_size(c_target, data_items).map(|size| (size, 0, false)))
    {
        let c_target_const_ptr = display_numeric_const_ptr(c_target);
        out.push_str(&format!(
            "{pad}{{ int64_t _prev = cobol_display_to_int64({c_target_const_ptr}, {disp_size});\n"
        ));
        out.push_str(&format!("{pad}int64_t _result = {result_expr};\n"));
        if let Some(max_val) =
            get_pic_max(target_name, data_items).or_else(|| display_numeric_max_value(disp_size))
        {
            out.push_str(&format!(
                "{pad}if (llabs(_result) > {max_val}) {{ _size_error = 1;\n"
            ));
            emit_store_display_numeric(
                out,
                &format!("{pad}    "),
                "_prev",
                c_target,
                disp_size,
                data_items,
            );
            out.push_str(&format!("{pad}}} else {{\n"));
            emit_store_display_numeric(
                out,
                &format!("{pad}    "),
                "_result",
                c_target,
                disp_size,
                data_items,
            );
            out.push_str(&format!("{pad}}}\n"));
        } else {
            emit_store_display_numeric(out, pad, "_result", c_target, disp_size, data_items);
        }
        out.push_str(&format!("{pad}}}\n"));
        return;
    }

    let current = if find_data_item_by_c_name(c_target, data_items)
        .or_else(|| find_data_item(c_target, data_items))
        .is_some_and(|item| item.is_numeric_edited)
    {
        let size = find_data_item_size(c_target, data_items);
        format!("cobol_func_numval((const uint8_t*){c_target}, {size})")
    } else {
        c_target.to_string()
    };

    out.push_str(&format!("{pad}{{ int64_t _prev = {current};\n"));
    out.push_str(&format!("{pad}int64_t _result = {result_expr};\n"));
    if let Some(max_val) = get_pic_max(target_name, data_items) {
        out.push_str(&format!(
            "{pad}if (llabs(_result) > {max_val}) {{ _size_error = 1; "
        ));
        emit_store_int(out, c_target, "_prev", data_items, "");
        out.push_str("} else { ");
        emit_store_int(out, c_target, "_result", data_items, "");
        out.push_str("}\n");
    } else {
        emit_store_int(out, c_target, "_result", data_items, pad);
    }
    out.push_str(&format!("{pad}}}\n"));
}

fn display_numeric_max_value(size: u32) -> Option<i64> {
    if size == 0 || size > 18 {
        return None;
    }
    let mut max = 1_i64;
    for _ in 0..size {
        max *= 10;
    }
    Some(max - 1)
}

fn numeric_edited_picture_scale(pic: &str) -> u32 {
    let bytes = pic.as_bytes();
    let actual_decimal = numeric_edited_actual_decimal_byte(bytes);
    let floating_symbol = numeric_edited_floating_symbol(bytes, actual_decimal);
    let mut i = 0usize;
    let mut in_frac = false;
    let mut scale = 0u32;

    while i < bytes.len() {
        match bytes[i].to_ascii_uppercase() {
            b'V' => {
                in_frac = true;
                i += 1;
            }
            b'.' | b',' if Some(bytes[i]) == actual_decimal => {
                in_frac = true;
                i += 1;
            }
            b'9' | b'Z' | b'*' => {
                let count = picture_repeat_count(bytes, &mut i);
                if in_frac {
                    scale += count;
                }
                i += 1;
            }
            ch if Some(ch) == floating_symbol => {
                let count = picture_repeat_count(bytes, &mut i);
                if in_frac {
                    scale += count;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    scale
}

fn numeric_edited_actual_decimal_byte(bytes: &[u8]) -> Option<u8> {
    let decimal_byte = if with_active_context(|ctx| ctx.decimal_point_is_comma()) {
        b','
    } else {
        b'.'
    };
    if bytes.contains(&decimal_byte) {
        Some(decimal_byte)
    } else {
        None
    }
}

fn picture_repeat_count(bytes: &[u8], i: &mut usize) -> u32 {
    if *i + 1 < bytes.len() && bytes[*i + 1] == b'(' {
        let start = *i + 2;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b')' {
            end += 1;
        }
        if end < bytes.len() {
            *i = end;
            return std::str::from_utf8(&bytes[start..end])
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(1);
        }
    }
    1
}

fn emit_move_to_numeric_edited(
    out: &mut String,
    from: &HirExpr,
    target_item: &HirDataItem,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let Some(pic) = &target_item.picture else {
        return false;
    };
    let escaped_pic = escape_c_string(pic);
    let pic_len = pic.len();
    let tgt_size = find_data_item_layout(c_target, data_items).item_len;
    let blank_when_zero_prefix = if target_item.blank_when_zero {
        format!(
            "if (_ned.value == 0) {{ memset((uint8_t*){c_target}, ' ', {tgt_size}); }} else {{ "
        )
    } else {
        String::new()
    };
    let blank_when_zero_suffix = if target_item.blank_when_zero { "}" } else { "" };

    if let Some(c_src) = numeric_edited_decimal_source_expr(from, data_items) {
        let normalize = normalize_numeric_edited_decimal_statement(pic);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _ned = {c_src}; {normalize}\
             {blank_when_zero_prefix}\
             char _ned_buf[256]; uint32_t _ned_len = cobol_decimal_to_display(\
             &_ned, (uint8_t*)_ned_buf, 256, (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
             cobol_move_string((const uint8_t*)_ned_buf, _ned_len, (uint8_t*){c_target}, {tgt_size}); \
             {blank_when_zero_suffix}}}\n"
        ));
        return true;
    }

    if let Some((value, scale)) = numeric_edited_scaled_source(from, data_items) {
        let normalize = normalize_numeric_edited_decimal_statement(pic);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _ned = {{ .value = ({value}), .scale = ({scale}), .size = {tgt_size}, .is_signed = 1 }}; \
             {normalize}\
             {blank_when_zero_prefix}\
             char _ned_buf[256]; uint32_t _ned_len = cobol_decimal_to_display(\
             &_ned, (uint8_t*)_ned_buf, 256, (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
             cobol_move_string((const uint8_t*)_ned_buf, _ned_len, (uint8_t*){c_target}, {tgt_size}); \
             {blank_when_zero_suffix}}}\n"
        ));
        return true;
    }

    false
}

fn numeric_edited_decimal_source_expr(
    from: &HirExpr,
    data_items: &[HirDataItem],
) -> Option<String> {
    if let Some(name) = expr_data_name(from) {
        if let Some(item) = find_data_item_by_name(name, data_items) {
            if let HirType::Numeric { decimal_places, .. } = &item.data_type {
                let c_source = emit_expr(from);
                if needs_decimal(&item.data_type) && !c_expr_is_display_numeric(&c_source) {
                    return Some(c_source);
                }
                if c_expr_is_display_numeric(&c_source) {
                    if let Some(size) = grp_display_size(&c_source, data_items) {
                        let ptr = display_numeric_const_ptr(&c_source);
                        return Some(format!(
                            "({{ CobolDecimal _src; cobol_decimal_from_int(cobol_display_to_int64({ptr}, {size}), {decimal_places}, &_src); _src; }})"
                        ));
                    }
                }
                if item.redefines.is_some() {
                    let ptr = display_numeric_const_ptr(&c_source);
                    let size = data_item_byte_size(&item.data_type);
                    return Some(format!(
                        "({{ CobolDecimal _src; cobol_decimal_from_int(cobol_display_to_int64({ptr}, {size}), {decimal_places}, &_src); _src; }})"
                    ));
                }
            }
        }
    }
    if is_decimal_expr(from, data_items) {
        Some(emit_expr(from))
    } else {
        None
    }
}

fn normalize_numeric_edited_decimal_statement(pic: &str) -> String {
    let integer_digits = numeric_edited_picture_integer_digits(pic);
    let scale = numeric_edited_picture_scale(pic);
    let total_digits = integer_digits + scale;
    let post_truncate = if total_digits > 0 && total_digits <= 18 {
        format!(
            "int64_t _limit = 1; for (int32_t _i = 0; _i < {total_digits}; _i++) _limit *= 10; \
             _ned.value = (_ned.value >= 0) ? (_ned.value % _limit) : -((-_ned.value) % _limit); "
        )
    } else {
        String::new()
    };
    format!(
        "int32_t _dd = {scale} - _ned.scale; \
         if (_dd > 0) _ned.value *= (int64_t)pow(10.0, _dd); \
         else if (_dd < 0) _ned.value /= (int64_t)pow(10.0, -_dd); \
         _ned.scale = {scale}; _ned.size = {total_digits}; {post_truncate}"
    )
}

fn numeric_edited_picture_integer_digits(pic: &str) -> u32 {
    let bytes = pic.as_bytes();
    let actual_decimal = numeric_edited_actual_decimal_byte(bytes);
    let floating_symbol = numeric_edited_floating_symbol(bytes, actual_decimal);
    let mut i = 0usize;
    let mut in_frac = false;
    let mut digits = 0u32;

    while i < bytes.len() {
        match bytes[i].to_ascii_uppercase() {
            b'V' => {
                in_frac = true;
                i += 1;
            }
            b'.' | b',' if Some(bytes[i]) == actual_decimal => {
                in_frac = true;
                i += 1;
            }
            b'9' | b'Z' | b'*' | b'P' => {
                let count = picture_repeat_count(bytes, &mut i);
                if !in_frac {
                    digits += count;
                }
                i += 1;
            }
            ch if Some(ch) == floating_symbol => {
                let count = picture_repeat_count(bytes, &mut i);
                if !in_frac {
                    digits += count;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    digits
}

fn numeric_edited_floating_symbol(bytes: &[u8], actual_decimal: Option<u8>) -> Option<u8> {
    let int_end = bytes
        .iter()
        .position(|b| *b == b'V' || Some(*b) == actual_decimal)
        .unwrap_or(bytes.len());
    let int_part = &bytes[..int_end];
    [b'$', b'+', b'-'].into_iter().find(|symbol| {
        let mut i = 0usize;
        let mut count = 0u32;
        while i < int_part.len() {
            if int_part[i].to_ascii_uppercase() == *symbol {
                count += picture_repeat_count(int_part, &mut i);
            }
            i += 1;
        }
        count > 1
    })
}

fn numeric_edited_scaled_source(
    from: &HirExpr,
    data_items: &[HirDataItem],
) -> Option<(String, String)> {
    match from {
        HirExpr::Literal(HirLiteral::Integer(n)) => Some((n.to_string(), "0".to_string())),
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let (scaled, scale) = parse_decimal_literal(d);
            Some((scaled.to_string(), scale.to_string()))
        }
        HirExpr::Literal(HirLiteral::String(s))
            if s.chars()
                .all(|ch| ch.is_ascii_digit() || ch == '+' || ch == '-') =>
        {
            let escaped = escape_c_string(s);
            Some((
                format!(
                    "cobol_func_numval((const uint8_t*)\"{escaped}\", {})",
                    s.len()
                ),
                "0".to_string(),
            ))
        }
        HirExpr::Literal(HirLiteral::Zero) => Some(("0".to_string(), "0".to_string())),
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            let c_expr = emit_expr(from);
            if expr_data_name(from)
                .and_then(|name| find_data_item_by_name(name, data_items))
                .is_some_and(|item| item.is_numeric_edited)
            {
                let src_ptr = format!("(const uint8_t*){}", c_ptr_expr(&c_expr, data_items));
                let src_len = find_data_item_storage_size(&c_expr, data_items);
                let parsed =
                    format!("({{ CobolDecimal _src; cobol_decimal_from_string({src_ptr}, {src_len}, &_src); _src; }})");
                return Some((format!("({parsed}.value)"), format!("({parsed}.scale)")));
            }
            if let Some((size, scale, _)) = display_numeric_c_expr_metadata(&c_expr, data_items) {
                return Some((
                    format!(
                        "cobol_display_to_int64({}, {size})",
                        display_numeric_const_ptr(&c_expr)
                    ),
                    scale.to_string(),
                ));
            }
            if is_alpha_expr(from, data_items) || is_group_expr(from, data_items) {
                let (src_ptr, src_len) = emit_alphanumeric_operand(from, data_items);
                if expr_data_name(from)
                    .and_then(|name| find_data_item_by_name(name, data_items))
                    .is_some_and(|item| item.is_numeric_edited)
                {
                    let parsed =
                        format!("({{ CobolDecimal _src; cobol_decimal_from_string({src_ptr}, {src_len}, &_src); _src; }})");
                    return Some((format!("({parsed}.value)"), format!("({parsed}.scale)")));
                }
                return Some((
                    format!("cobol_func_numval({src_ptr}, {src_len})"),
                    "0".to_string(),
                ));
            }
            let expr = emit_int_compatible_expr(from, data_items);
            Some((expr, "0".to_string()))
        }
        _ if expr_requires_double_precision(from, data_items) => {
            let expr = emit_expr_as_double(from);
            Some((
                format!("(int64_t)round(({expr}) * 1000000000.0)"),
                "9".to_string(),
            ))
        }
        _ => None,
    }
}

fn effective_perform_thru_paragraphs(
    paragraphs: &[HirParagraph],
    include_section_headers: bool,
) -> Vec<&HirParagraph> {
    let mut selected_sections = Vec::new();
    let mut effective = Vec::new();

    for paragraph in paragraphs {
        if !include_section_headers && matches!(paragraph.kind, HirParagraphKind::Section) {
            continue;
        }
        if let Some(section_id) = paragraph.section_id {
            if selected_sections.contains(&section_id) {
                continue;
            }
        }

        if matches!(paragraph.kind, HirParagraphKind::Section) {
            selected_sections.push(paragraph.id);
        }
        effective.push(paragraph);
    }

    effective
}

fn paragraph_group_end(paragraphs: &[HirParagraph], start: usize) -> usize {
    let paragraph = &paragraphs[start];
    if matches!(paragraph.kind, HirParagraphKind::Section) {
        let mut end = start + 1;
        while end < paragraphs.len() {
            if paragraphs[end].section_id == Some(paragraph.id) {
                end += 1;
                continue;
            }
            break;
        }
        return end;
    }

    start + 1
}

fn varying_target_c_expr(var: &str, var_expr: &HirExpr, until: &HirCondition) -> String {
    match var_expr {
        HirExpr::Subscript { .. } => return super::emit_expr(var_expr),
        HirExpr::DataRef(data_ref) if !data_ref.subscripts.is_empty() => {
            return super::emit_expr(var_expr);
        }
        _ => {}
    }
    find_subscripted_var_in_condition(until, var)
        .map(super::emit_expr)
        .unwrap_or_else(|| sanitize_name(var))
}

fn decimal_item_scale(name: &str, data_items: &[HirDataItem]) -> Option<u32> {
    find_data_item(name, data_items).and_then(|item| match item.data_type {
        HirType::Numeric { decimal_places, .. } => Some(decimal_places),
        HirType::Comp3 { decimal_places, .. } => Some(decimal_places),
        _ => None,
    })
}

fn decimal_expr_scale(expr: &HirExpr, data_items: &[HirDataItem]) -> Option<u32> {
    expr_data_name(expr)
        .and_then(|name| find_data_item_by_name(name, data_items))
        .or_else(|| expr_data_name(expr).and_then(|name| find_data_item(name, data_items)))
        .and_then(|item| match item.data_type {
            HirType::Numeric { decimal_places, .. } => Some(decimal_places),
            HirType::Comp3 { decimal_places, .. } => Some(decimal_places),
            _ => None,
        })
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

fn expr_mentions_var(expr: &HirExpr, var: &str) -> bool {
    match expr {
        HirExpr::DataRef(data_ref) => data_ref.name.name == var,
        HirExpr::Variable(name) => name == var,
        HirExpr::Subscript { variable, .. } => variable == var,
        HirExpr::UnaryOp { operand, .. } => expr_mentions_var(operand, var),
        HirExpr::BinaryOp { left, right, .. } => {
            expr_mentions_var(left, var) || expr_mentions_var(right, var)
        }
        HirExpr::FunctionCall { args, .. } => args.iter().any(|arg| expr_mentions_var(arg, var)),
        HirExpr::ReferenceModification { start, length, .. } => {
            expr_mentions_var(start, var)
                || length
                    .as_ref()
                    .is_some_and(|len| expr_mentions_var(len, var))
        }
        _ => false,
    }
}

fn condition_mentions_var(cond: &HirCondition, var: &str) -> bool {
    match cond {
        HirCondition::Compare { left, right, .. } => {
            expr_mentions_var(left, var) || expr_mentions_var(right, var)
        }
        HirCondition::ClassCondition { operand, .. } => expr_mentions_var(operand, var),
        HirCondition::And(a, b) | HirCondition::Or(a, b) => {
            condition_mentions_var(a, var) || condition_mentions_var(b, var)
        }
        HirCondition::Not(inner) => condition_mentions_var(inner, var),
    }
}

fn pow10_i64_literal(exp: u32) -> String {
    match exp {
        0 => "1LL".to_string(),
        1 => "10LL".to_string(),
        2 => "100LL".to_string(),
        3 => "1000LL".to_string(),
        4 => "10000LL".to_string(),
        5 => "100000LL".to_string(),
        6 => "1000000LL".to_string(),
        7 => "10000000LL".to_string(),
        8 => "100000000LL".to_string(),
        9 => "1000000000LL".to_string(),
        10 => "10000000000LL".to_string(),
        11 => "100000000000LL".to_string(),
        12 => "1000000000000LL".to_string(),
        13 => "10000000000000LL".to_string(),
        14 => "100000000000000LL".to_string(),
        15 => "1000000000000000LL".to_string(),
        16 => "10000000000000000LL".to_string(),
        17 => "100000000000000000LL".to_string(),
        18 => "1000000000000000000LL".to_string(),
        _ => format!("((int64_t)pow(10.0, {exp}))"),
    }
}

fn decimal_expr_as_scaled_int64(
    expr: &HirExpr,
    target_scale: u32,
    data_items: &[HirDataItem],
) -> Option<String> {
    match expr {
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            let c_expr = emit_expr(expr);
            if let Some((size, scale, _)) = display_numeric_c_expr_metadata(&c_expr, data_items) {
                if scale > target_scale {
                    return None;
                }
                let value = format!(
                    "cobol_display_to_int64({}, {size})",
                    display_numeric_const_ptr(&c_expr)
                );
                Some(if target_scale == scale {
                    value
                } else {
                    let factor = pow10_i64_literal(target_scale - scale);
                    format!("({value} * {factor})")
                })
            } else if is_decimal_expr(expr, data_items) {
                let scale = decimal_expr_scale(expr, data_items)?;
                if scale > target_scale {
                    return None;
                }
                let factor = pow10_i64_literal(target_scale - scale);
                Some(if target_scale == scale {
                    format!("{c_expr}.value")
                } else {
                    format!("({c_expr}.value * {factor})")
                })
            } else {
                let value = emit_int_compatible_expr(expr, data_items);
                let factor = pow10_i64_literal(target_scale);
                Some(format!("(({value}) * {factor})"))
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            let factor = pow10_i64_literal(target_scale);
            Some(format!("(({n}) * {factor})"))
        }
        HirExpr::Literal(HirLiteral::Decimal(_)) | HirExpr::UnaryOp { .. } => {
            if let Some((scaled, scale)) = signed_decimal_literal_expr(expr) {
                if scale > target_scale {
                    None
                } else if scale == target_scale {
                    Some(scaled.to_string())
                } else {
                    let factor = pow10_i64_literal(target_scale - scale);
                    Some(format!("(({scaled}) * {factor})"))
                }
            } else {
                match expr {
                    HirExpr::UnaryOp {
                        op: HirUnaryOp::Neg,
                        operand,
                    } => decimal_expr_as_scaled_int64(operand, target_scale, data_items)
                        .map(|inner| format!("(-({inner}))")),
                    _ => None,
                }
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = decimal_expr_as_scaled_int64(left, target_scale, data_items)?;
            let r = decimal_expr_as_scaled_int64(right, target_scale, data_items)?;
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                _ => return None,
            };
            Some(format!("(({l}) {op_str} ({r}))"))
        }
        _ => None,
    }
}

fn emit_fast_decimal_add_assign(
    out: &mut String,
    c_target: &str,
    target: &HirExpr,
    operand: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let display_target = display_numeric_c_expr_metadata(c_target, data_items);
    let target_scale = display_target
        .map(|(_, scale, _)| scale)
        .or_else(|| decimal_expr_scale(target, data_items));
    let target_scale = match target_scale {
        Some(scale) => scale,
        None => return false,
    };
    let scaled_operand = match decimal_expr_as_scaled_int64(operand, target_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    if let Some((target_size, _, _)) = display_target {
        let current = format!(
            "cobol_display_to_int64({}, {target_size})",
            display_numeric_const_ptr(c_target)
        );
        let result = format!("(({current}) + ({scaled_operand}))");
        emit_store_display_numeric(out, pad, &result, c_target, target_size, data_items);
    } else {
        out.push_str(&format!("{pad}{c_target}.value += ({scaled_operand});\n"));
    }
    true
}

fn emit_fast_decimal_sub_assign(
    out: &mut String,
    c_target: &str,
    target: &HirExpr,
    operand: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let display_target = display_numeric_c_expr_metadata(c_target, data_items);
    let target_scale = display_target
        .map(|(_, scale, _)| scale)
        .or_else(|| decimal_expr_scale(target, data_items));
    let target_scale = match target_scale {
        Some(scale) => scale,
        None => return false,
    };
    let scaled_operand = match decimal_expr_as_scaled_int64(operand, target_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    if let Some((target_size, _, _)) = display_target {
        let current = format!(
            "cobol_display_to_int64({}, {target_size})",
            display_numeric_const_ptr(c_target)
        );
        let result = format!("(({current}) - ({scaled_operand}))");
        emit_store_display_numeric(out, pad, &result, c_target, target_size, data_items);
    } else {
        out.push_str(&format!("{pad}{c_target}.value -= ({scaled_operand});\n"));
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn emit_fast_decimal_multiply_giving(
    out: &mut String,
    c_target: &str,
    target: &HirExpr,
    operand: &HirExpr,
    by_operand: Option<&HirExpr>,
    rounded: bool,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let target_scale = match decimal_expr_scale(target, data_items) {
        Some(scale) => scale,
        None => return false,
    };
    let by_operand = match by_operand {
        Some(expr) => expr,
        None => return false,
    };
    let left_scale = match decimal_expr_effective_scale(operand, data_items) {
        Some(scale) => scale,
        None => return false,
    };
    let right_scale = match decimal_expr_effective_scale(by_operand, data_items) {
        Some(scale) => scale,
        None => return false,
    };
    let left = match decimal_expr_as_scaled_int64(operand, left_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    let right = match decimal_expr_as_scaled_int64(by_operand, right_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    let combined_scale = left_scale + right_scale;
    let display_target = display_numeric_c_expr_metadata(c_target, data_items);
    if combined_scale > target_scale {
        let divisor = pow10_i64_literal(combined_scale - target_scale);
        let result = if rounded {
            format!(
                "(_raw >= 0 ? ((_raw + ({divisor} / 2)) / {divisor}) : ((_raw - ({divisor} / 2)) / {divisor}))"
            )
        } else {
            format!("(_raw / {divisor})")
        };
        if let Some((target_size, _, _)) = display_target {
            let target_ptr = display_numeric_ptr(c_target);
            out.push_str(&format!(
                "{pad}{{ __int128 _raw = (__int128)({left}) * (__int128)({right}); \
                 cobol_store_numeric_display((int64_t)({result}), {target_ptr}, {target_size}); }}\n"
            ));
        } else {
            out.push_str(&format!(
                "{pad}{{ if ({c_target}.size == 0 && {c_target}.scale == 0) {{ {c_target}.scale = {target_scale}; {c_target}.size = 18; {c_target}.is_signed = 1; }} \
                 __int128 _raw = (__int128)({left}) * (__int128)({right}); {c_target}.value = (int64_t)({result}); }}\n"
            ));
        }
    } else {
        let factor = pow10_i64_literal(target_scale - combined_scale);
        if let Some((target_size, _, _)) = display_target {
            let target_ptr = display_numeric_ptr(c_target);
            out.push_str(&format!(
                "{pad}{{ __int128 _raw = (__int128)({left}) * (__int128)({right}); \
                 cobol_store_numeric_display((int64_t)(_raw * {factor}), {target_ptr}, {target_size}); }}\n"
            ));
        } else {
            out.push_str(&format!(
                "{pad}{{ if ({c_target}.size == 0 && {c_target}.scale == 0) {{ {c_target}.scale = {target_scale}; {c_target}.size = 18; {c_target}.is_signed = 1; }} \
                 __int128 _raw = (__int128)({left}) * (__int128)({right}); {c_target}.value = (int64_t)(_raw * {factor}); }}\n"
            ));
        }
    }
    true
}

fn decimal_expr_effective_scale(expr: &HirExpr, data_items: &[HirDataItem]) -> Option<u32> {
    if let Some(scale) = decimal_expr_scale(expr, data_items) {
        return Some(scale);
    }
    if let Some((_, scale)) = signed_decimal_literal_expr(expr) {
        return Some(scale);
    }
    match expr {
        HirExpr::Literal(HirLiteral::Integer(_))
        | HirExpr::Literal(HirLiteral::Zero)
        | HirExpr::DataRef(_)
        | HirExpr::Variable(_)
        | HirExpr::Subscript { .. } => Some(0),
        _ => None,
    }
}

fn invert_compare_op(op: HirCompareOp) -> HirCompareOp {
    match op {
        HirCompareOp::Eq => HirCompareOp::Eq,
        HirCompareOp::Ne => HirCompareOp::Ne,
        HirCompareOp::Gt => HirCompareOp::Lt,
        HirCompareOp::Lt => HirCompareOp::Gt,
        HirCompareOp::Ge => HirCompareOp::Le,
        HirCompareOp::Le => HirCompareOp::Ge,
    }
}

fn compare_op_str(op: HirCompareOp) -> &'static str {
    match op {
        HirCompareOp::Eq => "==",
        HirCompareOp::Ne => "!=",
        HirCompareOp::Gt => ">",
        HirCompareOp::Lt => "<",
        HirCompareOp::Ge => ">=",
        HirCompareOp::Le => "<=",
    }
}

fn fast_decimal_varying_condition(
    var: &str,
    _c_var_target: &str,
    until: &HirCondition,
    data_items: &[HirDataItem],
) -> Option<String> {
    let target_scale = decimal_item_scale(var, data_items)?;
    match until {
        HirCondition::Compare { left, op, right } => {
            if expr_mentions_var(left, var) {
                let l = decimal_expr_as_scaled_int64(left, target_scale, data_items)?;
                let r = decimal_expr_as_scaled_int64(right, target_scale, data_items)?;
                Some(format!("(({l}) {} ({r}))", compare_op_str(*op)))
            } else if expr_mentions_var(right, var) {
                let l = decimal_expr_as_scaled_int64(left, target_scale, data_items)?;
                let r = decimal_expr_as_scaled_int64(right, target_scale, data_items)?;
                Some(format!(
                    "(({l}) {} ({r}))",
                    compare_op_str(invert_compare_op(*op))
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn emit_fast_decimal_varying_increment(
    out: &mut String,
    var: &str,
    c_var_target: &str,
    by: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let Some(target_scale) = decimal_item_scale(var, data_items) else {
        return false;
    };
    let Some(delta) = decimal_expr_as_scaled_int64(by, target_scale, data_items) else {
        return false;
    };
    out.push_str(&format!("{pad}{c_var_target}.value += ({delta});\n"));
    true
}

fn find_subscripted_var_in_condition<'a>(cond: &'a HirCondition, var: &str) -> Option<&'a HirExpr> {
    match cond {
        HirCondition::Compare { left, right, .. } => find_subscripted_var_in_expr(left, var)
            .or_else(|| find_subscripted_var_in_expr(right, var)),
        HirCondition::ClassCondition { operand, .. } => find_subscripted_var_in_expr(operand, var),
        HirCondition::Not(inner) => find_subscripted_var_in_condition(inner, var),
        HirCondition::And(a, b) | HirCondition::Or(a, b) => {
            find_subscripted_var_in_condition(a, var)
                .or_else(|| find_subscripted_var_in_condition(b, var))
        }
    }
}

fn find_subscripted_var_in_expr<'a>(expr: &'a HirExpr, var: &str) -> Option<&'a HirExpr> {
    match expr {
        HirExpr::Subscript { variable, .. }
            if variable.as_str() == var
                || sanitize_name(variable.as_str()) == sanitize_name(var) =>
        {
            Some(expr)
        }
        HirExpr::DataRef(data_ref)
            if !data_ref.subscripts.is_empty()
                && (data_ref.name.as_str() == var
                    || sanitize_name(data_ref.name.as_str()) == sanitize_name(var)) =>
        {
            Some(expr)
        }
        HirExpr::UnaryOp { operand, .. } => find_subscripted_var_in_expr(operand, var),
        HirExpr::BinaryOp { left, right, .. } => find_subscripted_var_in_expr(left, var)
            .or_else(|| find_subscripted_var_in_expr(right, var)),
        HirExpr::ReferenceModification { start, length, .. } => {
            find_subscripted_var_in_expr(start, var).or_else(|| {
                length
                    .as_ref()
                    .and_then(|len| find_subscripted_var_in_expr(len, var))
            })
        }
        HirExpr::FunctionCall { args, .. } => args
            .iter()
            .find_map(|arg| find_subscripted_var_in_expr(arg, var)),
        _ => None,
    }
}

fn emit_scaled_decimal_multiply_result(
    out: &mut String,
    target_expr: &str,
    decimal_expr: &str,
    rounded: bool,
    pad: &str,
) {
    out.push_str(&format!(
        "{pad}__int128 _raw = (__int128)({target_expr}) * (__int128){decimal_expr}.value;\n\
         {pad}int64_t _divisor = (int64_t)pow(10.0, {decimal_expr}.scale);\n"
    ));
    if rounded {
        out.push_str(&format!(
            "{pad}int64_t _result = (int64_t)((_raw >= 0) \
             ? ((_raw + (_divisor / 2)) / _divisor) \
             : ((_raw - (_divisor / 2)) / _divisor));\n"
        ));
    } else {
        out.push_str(&format!(
            "{pad}int64_t _result = (int64_t)(_raw / _divisor);\n"
        ));
    }
}

fn emit_rounded_decimal_multiply_by(
    out: &mut String,
    c_target: &str,
    operand: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if is_decimal_expr(operand, data_items) {
        let c_operand = emit_expr(operand);
        out.push_str(&format!(
            "{pad}{{ __int128 _raw = (__int128){c_target}.value * (__int128){c_operand}.value;\n\
             {pad}int64_t _divisor = (int64_t)pow(10.0, {c_operand}.scale);\n\
             {pad}{c_target}.value = (int64_t)((_raw >= 0) \
             ? ((_raw + (_divisor / 2)) / _divisor) \
             : ((_raw - (_divisor / 2)) / _divisor)); }}\n"
        ));
        return;
    }

    if let Some((scaled, scale)) = decimal_literal_parts(operand) {
        out.push_str(&format!(
            "{pad}{{ __int128 _raw = (__int128){c_target}.value * (__int128)({scaled});\n\
             {pad}int64_t _divisor = (int64_t)pow(10.0, {scale});\n\
             {pad}{c_target}.value = (int64_t)((_raw >= 0) \
             ? ((_raw + (_divisor / 2)) / _divisor) \
             : ((_raw - (_divisor / 2)) / _divisor)); }}\n"
        ));
        return;
    }

    if expr_is_scaled_display_numeric(operand, data_items) {
        out.push_str(&format!("{pad}{{ "));
        out.push_str(&decimal_init_statement("_mop", Some(operand), data_items));
        out.push_str(&format!(
            "__int128 _raw = (__int128){c_target}.value * (__int128)_mop.value; \
             int64_t _divisor = (int64_t)pow(10.0, _mop.scale); \
             {c_target}.value = (int64_t)((_raw >= 0) \
             ? ((_raw + (_divisor / 2)) / _divisor) \
             : ((_raw - (_divisor / 2)) / _divisor)); }}\n"
        ));
        return;
    }

    let c_operand = emit_int_compatible_expr(operand, data_items);
    out.push_str(&format!("{pad}{c_target}.value *= ({c_operand});\n"));
}

fn decimal_literal_parts(expr: &HirExpr) -> Option<(i64, u32)> {
    match expr {
        HirExpr::Literal(HirLiteral::Integer(n)) => Some((*n, 0)),
        HirExpr::Literal(HirLiteral::Decimal(d)) => Some(parse_decimal_literal(d)),
        HirExpr::UnaryOp {
            op: HirUnaryOp::Neg,
            operand,
        } => decimal_literal_parts(operand).map(|(value, scale)| (-value, scale)),
        _ => None,
    }
}

fn decimal_init_statement(
    var_name: &str,
    expr: Option<&HirExpr>,
    data_items: &[HirDataItem],
) -> String {
    let Some(expr) = expr else {
        return format!("CobolDecimal {var_name}; cobol_decimal_from_int(0, 0, &{var_name}); ");
    };
    if let Some((scaled, scale)) = decimal_literal_parts(expr) {
        format!("CobolDecimal {var_name}; cobol_decimal_from_int({scaled}, {scale}, &{var_name}); ")
    } else if let HirExpr::BinaryOp { op, left, right } = expr {
        if expr_contains_decimal(expr) && matches!(op, HirBinOp::Add | HirBinOp::Sub) {
            let left_var = format!("{var_name}_l");
            let right_var = format!("{var_name}_r");
            let combine = match op {
                HirBinOp::Add => decimal_add_exact_statement(var_name, &right_var),
                HirBinOp::Sub => decimal_subtract_exact_statement(var_name, &right_var),
                _ => unreachable!(),
            };
            format!(
                "{}{}CobolDecimal {var_name} = {left_var}; {combine}",
                decimal_init_statement(&left_var, Some(left), data_items),
                decimal_init_statement(&right_var, Some(right), data_items),
            )
        } else if expr_contains_decimal(expr) {
            let c_expr = emit_expr_as_double(expr);
            format!(
                "CobolDecimal {var_name} = {{ .value = 0, .scale = 9, .size = 18, .is_signed = 1 }}; cobol_decimal_from_double({c_expr}, &{var_name}); "
            )
        } else {
            let c_expr = emit_int_compatible_expr(expr, data_items);
            format!("CobolDecimal {var_name}; cobol_decimal_from_int({c_expr}, 0, &{var_name}); ")
        }
    } else if is_decimal_expr(expr, data_items) {
        let c_expr = emit_expr(expr);
        format!("CobolDecimal {var_name} = {c_expr}; ")
    } else {
        let raw_c_expr = emit_expr(expr);
        if let Some((size, scale, is_signed)) =
            display_numeric_c_expr_metadata(&raw_c_expr, data_items)
        {
            let c_ptr = display_numeric_const_ptr(&raw_c_expr);
            let signed = if is_signed { "true" } else { "false" };
            let raw_value = format!("cobol_display_to_int64({c_ptr}, {size})");
            let value = find_data_item_by_c_name(&raw_c_expr, data_items)
                .or_else(|| find_data_item(&raw_c_expr, data_items))
                .filter(|item| item.scale_adjustment > 0)
                .map_or(raw_value.clone(), |item| {
                    apply_scale_adjustment_to_read(&raw_value, item.scale_adjustment)
                });
            return format!(
                "CobolDecimal {var_name}; cobol_decimal_from_int({value}, {scale}, &{var_name}); {var_name}.size = {size}; {var_name}.scale = {scale}; {var_name}.is_signed = {signed}; "
            );
        }
        let c_expr = emit_int_compatible_expr(expr, data_items);
        format!("CobolDecimal {var_name}; cobol_decimal_from_int({c_expr}, 0, &{var_name}); ")
    }
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

fn decimal_rescale_to_target_statement(source: &str, target: &str, rounded: bool) -> String {
    let reduce = if rounded {
        format!(
            "_result = ({source}.value >= 0) ? (({source}.value + (_factor / 2)) / _factor) : (({source}.value - (_factor / 2)) / _factor);"
        )
    } else {
        format!("_result = {source}.value / _factor;")
    };
    format!(
        "int64_t _result = {source}.value; \
         if ({source}.scale > {target}.scale) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {source}.scale - {target}.scale; _i++) _factor *= 10; \
             {reduce} \
         }} else if ({source}.scale < {target}.scale) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {target}.scale - {source}.scale; _i++) _factor *= 10; \
             _result = {source}.value * _factor; \
         }} "
    )
}

fn decimal_rescale_to_scale_statement(source: &str, target_scale: u32, rounded: bool) -> String {
    let reduce = if rounded {
        format!(
            "_result = ({source}.value >= 0) ? (({source}.value + (_factor / 2)) / _factor) : (({source}.value - (_factor / 2)) / _factor);"
        )
    } else {
        format!("_result = {source}.value / _factor;")
    };
    format!(
        "int64_t _result = {source}.value; \
         if ({source}.scale > {target_scale}) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {source}.scale - {target_scale}; _i++) _factor *= 10; \
             {reduce} \
         }} else if ({source}.scale < {target_scale}) {{ \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < {target_scale} - {source}.scale; _i++) _factor *= 10; \
             _result = {source}.value * _factor; \
         }} "
    )
}

fn decimal_target_metadata_init_statement(
    target: &HirExpr,
    c_target: &str,
    data_items: &[HirDataItem],
) -> String {
    let scale = decimal_expr_scale(target, data_items).unwrap_or(0);
    format!(
        "if ({c_target}.size == 0 && {c_target}.scale == 0) {{ \
         {c_target}.scale = {scale}; {c_target}.size = 18; {c_target}.is_signed = 1; }} "
    )
}

fn emit_decimal_divide_to_target_statement(
    pad: &str,
    target: &str,
    init_a: &str,
    init_b: &str,
    rounded: bool,
    has_size_error: bool,
    max_val: Option<i64>,
) -> String {
    let guard_digit = if rounded {
        "_target_scale = _target_meta.scale + 1;"
    } else {
        "_target_scale = _target_meta.scale;"
    };
    let scale_dividend = "if (_da.scale < _target_scale) { \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < _target_scale - _da.scale; _i++) _factor *= 10; \
             _da.value *= _factor; \
             _da.scale = _target_scale; \
         }";
    let store_result = if has_size_error {
        let overflow_check = max_val
            .map(|max_val| format!(" || llabs(_result) > {max_val}"))
            .unwrap_or_default();
        format!(
            "if (_db.value == 0) {{ _size_error = 1; }} \
             else {{ \
                 cobol_decimal_div(&_da, &_db, &_dr); \
                 {} \
                 if (0{overflow_check}) {{ _size_error = 1; }} \
                 else {{ \
                     {target}.value = _result; \
                     {target}.scale = _target_meta.scale; \
                     {target}.size = _target_meta.size; \
                     {target}.is_signed = _target_meta.is_signed || _dr.is_signed; \
                 }} \
             }}",
            decimal_rescale_to_target_statement("_dr", "_target_meta", rounded)
        )
    } else {
        format!(
            "cobol_decimal_div(&_da, &_db, &_dr); \
             {} \
             {target}.value = _result; \
             {target}.scale = _target_meta.scale; \
             {target}.size = _target_meta.size; \
             {target}.is_signed = _target_meta.is_signed || _dr.is_signed;",
            decimal_rescale_to_target_statement("_dr", "_target_meta", rounded)
        )
    };
    format!(
        "{pad}{{ CobolDecimal _target_meta = {target}; \
         int32_t _target_scale; \
         {init_a} {init_b} \
         {guard_digit} \
         {scale_dividend} \
         _da.size = 18; \
         _db.size = 18; \
         CobolDecimal _dr = _target_meta; \
        {store_result} }}\n"
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_decimal_divide_to_display_statement(
    pad: &str,
    target: &str,
    init_a: &str,
    init_b: &str,
    rounded: bool,
    has_size_error: bool,
    max_val: Option<i64>,
    data_items: &[HirDataItem],
) -> String {
    let Some((target_size, target_scale, target_signed)) =
        display_numeric_c_expr_metadata(target, data_items)
    else {
        return String::new();
    };
    let target_scale_adjustment = find_data_item_by_c_name(target, data_items)
        .or_else(|| find_data_item(target, data_items))
        .filter(|item| item.scale_adjustment > 0)
        .map_or(0, |item| item.scale_adjustment);
    let target_meta_scale = target_scale as i32 - target_scale_adjustment;
    let target_ptr = display_numeric_ptr(target);
    let target_signed = if target_signed { "1" } else { "0" };
    let guard_digit = if rounded {
        "_target_scale = _target_meta.scale + 1;"
    } else {
        "_target_scale = _target_meta.scale;"
    };
    let scale_dividend = "if (_da.scale < _target_scale) { \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < _target_scale - _da.scale; _i++) _factor *= 10; \
             _da.value *= _factor; \
             _da.scale = _target_scale; \
         }";
    let rescale = decimal_rescale_to_target_statement("_dr", "_target_meta", rounded);
    let store_result = if has_size_error {
        let overflow_check = max_val
            .map(|max_val| format!(" || llabs(_result) > {max_val}"))
            .unwrap_or_default();
        format!(
            "if (_db.value == 0) {{ _size_error = 1; }} \
             else {{ \
                 cobol_decimal_div(&_da, &_db, &_dr); \
                 {rescale} \
                 if (0{overflow_check}) {{ _size_error = 1; }} \
                 else {{ cobol_store_numeric_display(_result, {target_ptr}, {target_size}); }} \
             }}"
        )
    } else {
        format!(
            "cobol_decimal_div(&_da, &_db, &_dr); \
             {rescale} \
             cobol_store_numeric_display(_result, {target_ptr}, {target_size});"
        )
    };
    format!(
        "{pad}{{ CobolDecimal _target_meta = {{ .value = 0, .scale = {target_meta_scale}, .size = {target_size}, .is_signed = {target_signed} }}; \
         int32_t _target_scale; \
         {init_a} {init_b} \
         {guard_digit} \
         {scale_dividend} \
         _da.size = 18; \
         _db.size = 18; \
         CobolDecimal _dr = _target_meta; \
         {store_result} }}\n"
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_decimal_divide_to_numeric_edited_statement(
    pad: &str,
    target: &str,
    target_item: &HirDataItem,
    init_a: &str,
    init_b: &str,
    rounded: bool,
    has_size_error: bool,
    max_int: Option<i64>,
) -> String {
    let Some(pic) = &target_item.picture else {
        return String::new();
    };
    let escaped_pic = escape_c_string(pic);
    let pic_len = pic.len();
    let target_size = data_item_byte_size(&target_item.data_type);
    let scale = numeric_edited_picture_scale(pic);
    let total_digits = numeric_edited_picture_integer_digits(pic) + scale;
    let guard_digit = if rounded {
        "_target_scale = _target_meta.scale + 1;"
    } else {
        "_target_scale = _target_meta.scale;"
    };
    let scale_dividend = "if (_da.scale < _target_scale) { \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < _target_scale - _da.scale; _i++) _factor *= 10; \
             _da.value *= _factor; \
             _da.scale = _target_scale; \
         }";
    let overflow_check = max_int
        .map(|max_int| {
            format!(
                "int64_t _max_factor = 1; \
                 for (int32_t _i = 0; _i < _target_meta.scale; _i++) _max_factor *= 10; \
                 int64_t _max_scaled = ({max_int} * _max_factor) + (_max_factor - 1); \
                 if (llabs(_result) > _max_scaled) {{ _size_error = 1; }} else "
            )
        })
        .unwrap_or_default();
    let store_result = if has_size_error {
        format!(
            "if (_db.value == 0) {{ _size_error = 1; }} \
             else {{ \
                 cobol_decimal_div(&_da, &_db, &_dr); \
                 {} \
                 {overflow_check}{{ \
                     CobolDecimal _ned = {{ .value = _result, .scale = _target_meta.scale, \
                         .size = _target_meta.size, .is_signed = _dr.is_signed }}; \
                     char _ned_buf[256]; \
                     uint32_t _ned_len = cobol_decimal_to_display(&_ned, (uint8_t*)_ned_buf, 256, \
                         (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
                     cobol_move_string((const uint8_t*)_ned_buf, _ned_len, \
                         (uint8_t*){target}, {target_size}); \
                 }} \
             }}",
            decimal_rescale_to_target_statement("_dr", "_target_meta", rounded)
        )
    } else {
        format!(
            "cobol_decimal_div(&_da, &_db, &_dr); \
             {} \
             {{ CobolDecimal _ned = {{ .value = _result, .scale = _target_meta.scale, \
                  .size = _target_meta.size, .is_signed = _dr.is_signed }}; \
                char _ned_buf[256]; \
                uint32_t _ned_len = cobol_decimal_to_display(&_ned, (uint8_t*)_ned_buf, 256, \
                    (const uint8_t*)\"{escaped_pic}\", {pic_len}); \
                cobol_move_string((const uint8_t*)_ned_buf, _ned_len, \
                    (uint8_t*){target}, {target_size}); }}",
            decimal_rescale_to_target_statement("_dr", "_target_meta", rounded)
        )
    };
    format!(
        "{pad}{{ CobolDecimal _target_meta = {{ .value = 0, .scale = {scale}, \
         .size = {total_digits}, .is_signed = 1 }}; \
         int32_t _target_scale; \
         {init_a} {init_b} \
         {guard_digit} \
         {scale_dividend} \
         _da.size = 18; \
         _db.size = 18; \
         CobolDecimal _dr = _target_meta; \
         {store_result} }}\n"
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_divide_remainder_from_quotient(
    out: &mut String,
    pad: &str,
    quotient_expr: &HirExpr,
    c_quotient: &str,
    rounded: bool,
    remainder_expr: &HirExpr,
    c_remainder: &str,
    has_size_error: bool,
    data_items: &[HirDataItem],
) {
    let quotient_scale = decimal_expr_scale(quotient_expr, data_items)
        .or_else(|| {
            display_numeric_c_expr_metadata(c_quotient, data_items).map(|(_, scale, _)| scale)
        })
        .or_else(|| {
            find_data_item_by_c_name(c_quotient, data_items)
                .or_else(|| find_data_item(c_quotient, data_items))
                .filter(|item| item.is_numeric_edited)
                .and_then(|item| item.picture.as_deref().map(numeric_edited_picture_scale))
        })
        .unwrap_or(0);
    let guard_scale = if rounded {
        quotient_scale + 1
    } else {
        quotient_scale
    };
    let store_pad = "";
    let store_remainder = if let Some(item) = find_data_item_by_c_name(c_remainder, data_items)
        .or_else(|| find_data_item(c_remainder, data_items))
        .filter(|item| item.is_numeric_edited)
    {
        let mut s = String::new();
        emit_store_decimal_to_numeric_edited(&mut s, c_remainder, "_rrem", item, false, store_pad);
        format!("{{ {s} }}")
    } else if target_expr_is_decimal(remainder_expr, c_remainder, data_items) {
        let rem_scale = decimal_expr_scale(remainder_expr, data_items).unwrap_or(0);
        format!(
            "{{ {} {c_remainder}.value = _result; {c_remainder}.scale = {rem_scale}; }}",
            decimal_rescale_to_scale_statement("_rrem", rem_scale, false)
        )
    } else {
        let mut s = String::new();
        s.push_str("{ ");
        s.push_str(&decimal_rescale_to_scale_statement("_rrem", 0, false));
        s.push(' ');
        emit_store_int(&mut s, c_remainder, "_result", data_items, "");
        s.push_str(" }");
        s
    };
    let guard_open = if has_size_error {
        "if (!_size_error) "
    } else {
        ""
    };
    out.push_str(&format!(
        "{pad}{guard_open}{{ CobolDecimal _rda = _dg_into; CobolDecimal _rdb = _dg_operand; \
         int32_t _target_scale = {guard_scale}; \
         if (_rda.scale < _target_scale) {{ int64_t _factor = 1; \
             for (int32_t _i = 0; _i < _target_scale - _rda.scale; _i++) _factor *= 10; \
             _rda.value *= _factor; _rda.scale = _target_scale; }} \
         CobolDecimal _rq_meta = {{ .value = 0, .scale = {quotient_scale}, .size = 18, .is_signed = 1 }}; \
         CobolDecimal _rq_div = _rq_meta; cobol_decimal_div(&_rda, &_rdb, &_rq_div); \
         {} \
         CobolDecimal _rq = {{ .value = _result, .scale = {quotient_scale}, .size = 18, .is_signed = _rq_div.is_signed }}; \
         CobolDecimal _rprod = {{ .value = (int64_t)((__int128)_rdb.value * (__int128)_rq.value), \
             .scale = _rdb.scale + _rq.scale, .size = 18, .is_signed = _rdb.is_signed || _rq.is_signed }}; \
         CobolDecimal _rrem = _dg_into; {} {store_remainder} }}\n",
        decimal_rescale_to_target_statement("_rq_div", "_rq_meta", false),
        decimal_subtract_exact_statement("_rrem", "_rprod")
    ));
}

#[allow(clippy::too_many_arguments)]
fn emit_decimal_divide_to_int_target(
    out: &mut String,
    pad: &str,
    target: &str,
    init_a: &str,
    init_b: &str,
    rounded: bool,
    has_size_error: bool,
    max_val: Option<i64>,
    data_items: &[HirDataItem],
) {
    let guard_digit = if rounded {
        "_target_scale = _target_meta.scale + 1; \
         if (_da.scale < _target_scale) { \
             int64_t _factor = 1; \
             for (int32_t _i = 0; _i < _target_scale - _da.scale; _i++) _factor *= 10; \
             _da.value *= _factor; \
             _da.scale = _target_scale; \
         }"
    } else {
        ""
    };
    out.push_str(&format!(
        "{pad}{{ CobolDecimal _target_meta = {{ .value = 0, .scale = 0, \
         .size = 18, .is_signed = 1 }}; \
         int32_t _target_scale; \
         {init_a} {init_b} \
         {guard_digit} \
         _da.size = 18; \
         _db.size = 18; \
         CobolDecimal _dr = _target_meta; "
    ));
    let compute = format!(
        "cobol_decimal_div(&_da, &_db, &_dr); {}",
        decimal_rescale_to_target_statement("_dr", "_target_meta", rounded)
    );
    if has_size_error {
        out.push_str(&format!(
            "if (_db.value == 0) {{ _size_error = 1; }} else {{ {compute} "
        ));
        if let Some(max_val) = max_val {
            out.push_str(&format!(
                "if (llabs(_result) > {max_val}) {{ _size_error = 1; }} else {{ "
            ));
            emit_store_int(out, target, "_result", data_items, "");
            out.push_str("} ");
        } else {
            emit_store_int(out, target, "_result", data_items, "");
        }
        out.push_str("} }\n");
    } else {
        out.push_str(&compute);
        emit_store_int(out, target, "_result", data_items, "");
        out.push_str("}\n");
    }
}

fn decimal_subtract_exact_statement(acc: &str, rhs: &str) -> String {
    format!(
        "{{ int32_t _scale = {acc}.scale > {rhs}.scale ? {acc}.scale : {rhs}.scale; \
           __int128 _av = {acc}.value; \
           for (int32_t _i = 0; _i < _scale - {acc}.scale; _i++) _av *= 10; \
           __int128 _bv = {rhs}.value; \
           for (int32_t _i = 0; _i < _scale - {rhs}.scale; _i++) _bv *= 10; \
           {acc}.value = (int64_t)(_av - _bv); \
           {acc}.scale = _scale; \
           {acc}.size = {acc}.size > {rhs}.size ? {acc}.size : {rhs}.size; \
           {acc}.is_signed = {acc}.is_signed || {rhs}.is_signed; }} "
    )
}

fn numeric_edited_integer_max(pic: &str) -> Option<i64> {
    let mut int_digits = 0u32;
    for ch in pic.chars() {
        let ch = ch.to_ascii_uppercase();
        if ch == '.' || ch == 'V' {
            break;
        }
        if matches!(ch, '9' | 'Z' | '*' | '0') {
            int_digits += 1;
        }
    }
    if int_digits == 0 || int_digits >= 19 {
        None
    } else {
        Some(10_i64.pow(int_digits) - 1)
    }
}

/// Emit ON SIZE ERROR / NOT ON SIZE ERROR handlers.
///
/// Uses a simplified approach: the arithmetic is already emitted, so we
/// emit the NOT ON SIZE ERROR body unconditionally and add a TODO comment
/// for actual overflow detection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_on_size_error(
    out: &mut String,
    on_size_error: &[HirStatement],
    not_on_size_error: &[HirStatement],
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    if on_size_error.is_empty() && not_on_size_error.is_empty() {
        return;
    }
    let pad = "    ".repeat(indent);
    // The caller is responsible for setting `_size_error` flag before calling this.
    // We emit: if (_size_error) { ON SIZE ERROR body } else { NOT ON SIZE ERROR body }
    if !on_size_error.is_empty() || !not_on_size_error.is_empty() {
        out.push_str(&format!("{pad}if (_size_error) {{\n"));
        for s in on_size_error {
            emit_statement(
                out,
                s,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent + 1,
            );
        }
        if !not_on_size_error.is_empty() {
            out.push_str(&format!("{pad}}} else {{\n"));
            for s in not_on_size_error {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

/// Emit ON EXCEPTION / NOT ON EXCEPTION handlers for CALL.
///
/// Uses `_call_failed` flag (declared in caller scope) to branch.
/// Currently the flag is always 0 (success) since we don't yet detect
/// dynamic-link failures, but the code path is now reachable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_on_exception(
    out: &mut String,
    on_exception: &[HirStatement],
    not_on_exception: &[HirStatement],
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    if on_exception.is_empty() && not_on_exception.is_empty() {
        return;
    }
    let pad = "    ".repeat(indent);
    if !on_exception.is_empty() || !not_on_exception.is_empty() {
        out.push_str(&format!("{pad}if (_call_failed) {{\n"));
        for s in on_exception {
            emit_statement(
                out,
                s,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent + 1,
            );
        }
        if !not_on_exception.is_empty() {
            out.push_str(&format!("{pad}}} else {{\n"));
            for s in not_on_exception {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

fn emit_validate_statement(out: &mut String, target: &str, data_items: &[HirDataItem], pad: &str) {
    let c_target = sanitize_name(target);
    let Some(item) = find_data_item_by_c_name(&c_target, data_items)
        .or_else(|| find_data_item(&c_target, data_items))
    else {
        out.push_str(&format!(
            "{pad}cobol_validate(\"{c_target}\"); /* VALIDATE */\n"
        ));
        return;
    };

    emit_validate_item_recursive(out, target, &c_target, item, data_items, pad);
}

fn emit_validate_item_recursive(
    out: &mut String,
    target_name: &str,
    c_target: &str,
    item: &HirDataItem,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let picture = item.picture.as_deref().unwrap_or("");
    let escaped_pic = escape_c_string(picture);
    let pic_len = picture.len();
    let escaped_target_name = escape_c_string(target_name);
    let (ptr_expr, len_expr, kind) = validate_storage_args(&c_target, item, data_items);
    out.push_str(&format!(
        "{pad}if (cobol_validate_item(\"{escaped_target_name}\", {ptr_expr}, {len_expr}, {kind}, (const uint8_t*)\"{escaped_pic}\", {pic_len}) != 0) {{ cobol_raise(\"EC-DATA-INCOMPATIBLE\"); }} /* VALIDATE */\n"
    ));
    emit_validate_value_constraints(out, &c_target, item, data_items, pad);

    if let HirType::Group { members, .. } = &item.data_type {
        for member in members {
            if member.redefines.is_some() || member.renames.is_some() {
                continue;
            }
            let member_c_name = format!("{c_target}__{}", sanitize_name(&member.name));
            let member_target_name = format!("{target_name}.{}", member.name);
            emit_validate_item_recursive(
                out,
                &member_target_name,
                &member_c_name,
                member,
                data_items,
                pad,
            );
        }
    }
}

fn emit_validate_value_constraints(
    out: &mut String,
    c_target: &str,
    item: &HirDataItem,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if item.validation_values.is_empty() {
        return;
    }

    let is_alpha = matches!(item.data_type, HirType::Alphanumeric { .. });
    let is_numeric = matches!(
        item.data_type,
        HirType::Numeric { .. } | HirType::Comp3 { .. } | HirType::Binary { .. } | HirType::Index
    ) || display_numeric_c_expr_metadata(c_target, data_items).is_some();

    let mut checks = Vec::new();
    for value in &item.validation_values {
        match value {
            cobol_hir::HirValidationValue::Single(lit) if is_alpha => {
                if let Some((literal, len)) = validate_string_literal(lit) {
                    checks.push(format!(
                        "cobol_compare_alphanumeric((const uint8_t*){c_target}, {}, (const uint8_t*)\"{literal}\", {len}) == 0",
                        data_item_byte_size(&item.data_type)
                    ));
                }
            }
            cobol_hir::HirValidationValue::Single(lit) if is_numeric => {
                if let Some(value) = validate_integral_literal(lit) {
                    let expr = emit_numeric_expr_for_var(c_target, data_items);
                    checks.push(format!("({expr}) == {value}"));
                }
            }
            cobol_hir::HirValidationValue::Range { from, to } if is_numeric => {
                if let (Some(from), Some(to)) = (
                    validate_integral_literal(from),
                    validate_integral_literal(to),
                ) {
                    let expr = emit_numeric_expr_for_var(c_target, data_items);
                    checks.push(format!("(({expr}) >= {from} && ({expr}) <= {to})"));
                }
            }
            _ => {}
        }
    }

    if checks.is_empty() {
        return;
    }

    out.push_str(&format!(
        "{pad}if (!({})) {{ cobol_raise(\"EC-DATA-INCOMPATIBLE\"); }} /* VALIDATE VALUE */\n",
        checks.join(" || ")
    ));
}

fn validate_string_literal(lit: &HirLiteral) -> Option<(String, usize)> {
    match lit {
        HirLiteral::String(value) => Some((escape_c_string(value), value.len())),
        HirLiteral::Space => Some((" ".to_string(), 1)),
        HirLiteral::Quote => Some(("\\\"".to_string(), 1)),
        _ => None,
    }
}

fn validate_integral_literal(lit: &HirLiteral) -> Option<i64> {
    match lit {
        HirLiteral::Integer(value) => Some(*value),
        HirLiteral::Zero => Some(0),
        HirLiteral::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn validate_storage_args(
    c_target: &str,
    item: &HirDataItem,
    data_items: &[HirDataItem],
) -> (String, String, u32) {
    match &item.data_type {
        HirType::Alphanumeric { size } => (format!("(const void*){c_target}"), size.to_string(), 0),
        HirType::National { size } => (
            format!("(const void*){c_target}"),
            format!("{}", size * 2),
            0,
        ),
        HirType::Group { .. } => (
            format!("(const void*){c_target}._bytes"),
            format!("sizeof({c_target}._bytes)"),
            0,
        ),
        _ if item.is_numeric_edited || validate_numeric_uses_display_storage(item, data_items) => (
            format!("(const void*){c_target}"),
            find_data_item_size(c_target, data_items).to_string(),
            1,
        ),
        HirType::Numeric { decimal_places, .. } | HirType::Comp3 { decimal_places, .. }
            if *decimal_places > 0 =>
        {
            (
                format!("(const void*)&{c_target}"),
                format!("sizeof({c_target})"),
                3,
            )
        }
        HirType::Numeric { .. }
        | HirType::Comp3 { .. }
        | HirType::Binary { .. }
        | HirType::Index => (
            format!("(const void*)&{c_target}"),
            format!("sizeof({c_target})"),
            2,
        ),
        HirType::Boolean | HirType::FloatShort | HirType::FloatLong | HirType::FloatExtended => (
            format!("(const void*)&{c_target}"),
            format!("sizeof({c_target})"),
            0,
        ),
        HirType::Pointer => (
            format!("(const void*)&{c_target}"),
            format!("sizeof({c_target})"),
            0,
        ),
    }
}

fn validate_numeric_uses_display_storage(item: &HirDataItem, data_items: &[HirDataItem]) -> bool {
    if !matches!(item.data_type, HirType::Numeric { .. }) {
        return false;
    }
    item.sign.is_some_and(|sign| sign.separate)
        || data_items.iter().any(|other| {
            other
                .redefines
                .as_ref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&item.name))
                && matches!(
                    other.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. }
                )
        })
}

/// Emit an expression as a numeric C value, auto-converting CobolDecimal
/// variables to int64 using the DECIMAL_NAMES thread-local set.
/// Used for function call arguments where `(double)CobolDecimal` casts are invalid.
pub(crate) fn emit_expr_as_numeric(expr: &HirExpr) -> String {
    with_active_context(|ctx| emit_expr_as_numeric_with_ctx(expr, ctx))
}

pub(crate) fn emit_expr_as_numeric_with_ctx(expr: &HirExpr, ctx: &CodegenContext) -> String {
    let emit_expr = |expr| super::emit_expr_with_ctx(expr, ctx);
    let emit_expr_as_numeric = |expr| super::emit_expr_as_numeric_with_ctx(expr, ctx);
    match expr {
        HirExpr::DataRef(data_ref) => {
            let c_name = emit_expr(expr);
            let base_name = data_name_to_c_name(&data_ref.name);
            let leaf_name = extract_leaf_member(&c_name);
            let is_dec = ctx.is_decimal_name(&base_name) || ctx.is_decimal_name(leaf_name);
            let is_grp = ctx.is_group_name(&base_name) || ctx.is_group_name(leaf_name);
            let is_alpha = ctx.is_alpha_name(&base_name) || ctx.is_alpha_name(leaf_name);
            let disp_size = grp_display_size(&c_name, &[])
                .or_else(|| ctx.display_numeric_size(&base_name))
                .or_else(|| {
                    if c_name.contains("__") {
                        None
                    } else {
                        ctx.display_numeric_size(leaf_name)
                    }
                });
            if let Some(size) = disp_size {
                let c_name_ptr = display_numeric_const_ptr(&c_name);
                format!("cobol_display_to_int64({c_name_ptr}, {size})")
            } else if is_dec {
                format!("cobol_decimal_to_int64(&{c_name})")
            } else if is_alpha {
                let size = ctx
                    .data_item_size(&base_name)
                    .or_else(|| ctx.data_item_size(leaf_name))
                    .unwrap_or(1);
                let ptr = if size == 1 {
                    format!("(const uint8_t*)&{c_name}")
                } else {
                    format!("(const uint8_t*){c_name}")
                };
                format!("cobol_func_numval({ptr}, {size})")
            } else if is_grp {
                "((int64_t)0)".to_string()
            } else {
                c_name
            }
        }
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            let c_name = match expr {
                HirExpr::Variable(_) => data_name_to_c_name(name),
                _ => emit_expr(expr),
            };
            let base_name = sanitize_name(name.as_str());
            let leaf_name = extract_leaf_member(&c_name);
            let is_dec = ctx.is_decimal_name(&base_name) || ctx.is_decimal_name(leaf_name);
            let is_grp = ctx.is_group_name(&base_name) || ctx.is_group_name(leaf_name);
            let is_alpha = ctx.is_alpha_name(&base_name) || ctx.is_alpha_name(leaf_name);
            let disp_size = grp_display_size(&c_name, &[])
                .or_else(|| ctx.display_numeric_size(&c_name))
                .or_else(|| {
                    if c_name.contains("__") {
                        None
                    } else {
                        ctx.display_numeric_size(&base_name)
                            .or_else(|| ctx.display_numeric_size(leaf_name))
                    }
                });
            if let Some(size) = disp_size {
                let c_name_ptr = display_numeric_const_ptr(&c_name);
                format!("cobol_display_to_int64({c_name_ptr}, {size})")
            } else if is_dec {
                format!("cobol_decimal_to_int64(&{c_name})")
            } else if is_alpha {
                let size = ctx
                    .data_item_size(&base_name)
                    .or_else(|| ctx.data_item_size(leaf_name))
                    .unwrap_or(1);
                let ptr = if size == 1 {
                    format!("(const uint8_t*)&{c_name}")
                } else {
                    format!("(const uint8_t*){c_name}")
                };
                format!("cobol_func_numval({ptr}, {size})")
            } else if is_grp {
                // Group variables are C unions; cast to 0 in numeric context
                // (groups used in arithmetic are unusual; default to 0).
                "((int64_t)0)".to_string()
            } else {
                c_name
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
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
        _ => emit_expr(expr),
    }
}

/// Emit an expression as a `double`, preserving decimal fractional parts.
/// Used for math intrinsic function arguments (ACOS, ASIN, COS, SIN, TAN,
/// LOG, SQRT, etc.) where truncating to int64 loses precision.
pub(crate) fn emit_expr_as_double(expr: &HirExpr) -> String {
    with_active_context(|ctx| emit_expr_as_double_with_ctx(expr, ctx))
}

pub(crate) fn emit_expr_as_double_with_ctx(expr: &HirExpr, ctx: &CodegenContext) -> String {
    let emit_expr_as_numeric = |expr| super::emit_expr_as_numeric_with_ctx(expr, ctx);
    let emit_expr_as_double = |expr| super::emit_expr_as_double_with_ctx(expr, ctx);
    match expr {
        HirExpr::FunctionCall { name, args }
            if matches!(name.to_ascii_uppercase().as_str(), "NUMVAL" | "NUMVAL-C") =>
        {
            if let Some(arg) = args.first() {
                let (c_src, c_len) = emit_string_func_arg(arg);
                format!("cobol_func_numval_double((const uint8_t*){c_src}, {c_len})")
            } else {
                "0.0".to_string()
            }
        }
        HirExpr::DataRef(_) | HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            let c_name = super::emit_expr_with_ctx(expr, ctx);
            let base_name = expr_data_name(expr)
                .map(data_name_to_c_name)
                .unwrap_or_default();
            let leaf_name = extract_leaf_member(&c_name);
            let is_dec = (!base_name.is_empty() && ctx.is_decimal_name(&base_name))
                || ctx.is_decimal_name(leaf_name);
            let disp_size = grp_display_size(&c_name, &[]).or_else(|| {
                if base_name.is_empty() {
                    ctx.display_numeric_size(leaf_name)
                } else {
                    ctx.display_numeric_size(&base_name)
                        .or_else(|| ctx.display_numeric_size(leaf_name))
                }
            });
            if let Some(size) = disp_size {
                let c_name_ptr = display_numeric_const_ptr(&c_name);
                let scale = if base_name.is_empty() {
                    ctx.display_numeric_scale(leaf_name)
                } else {
                    ctx.display_numeric_scale(&base_name)
                        .or_else(|| ctx.display_numeric_scale(leaf_name))
                }
                .unwrap_or(0);
                if scale > 0 {
                    format!("((double)cobol_display_to_int64({c_name_ptr}, {size}) / pow(10.0, {scale}))")
                } else {
                    format!("(double)cobol_display_to_int64({c_name_ptr}, {size})")
                }
            } else if is_dec {
                format!("cobol_decimal_to_double(&{c_name})")
            } else if (!base_name.is_empty() && ctx.is_group_name(&base_name))
                || ctx.is_group_name(leaf_name)
            {
                let size = ctx
                    .data_item_size(&base_name)
                    .or_else(|| ctx.data_item_size(leaf_name))
                    .unwrap_or(0);
                format!("(double)cobol_func_numval((const uint8_t*)&{c_name}, {size})")
            } else {
                if !base_name.is_empty()
                    && (c_name.contains('[') || c_name.contains(".members._m_"))
                    && (ctx.is_alpha_name(&base_name) || ctx.is_alpha_name(leaf_name))
                {
                    let size = ctx
                        .data_item_size(&base_name)
                        .or_else(|| ctx.data_item_size(leaf_name))
                        .unwrap_or(0);
                    format!("(double)cobol_func_numval((const uint8_t*){c_name}, {size})")
                } else {
                    format!("(double){c_name}")
                }
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = emit_expr_as_double(left);
            let r = emit_expr_as_double(right);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("pow({l}, {r})"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_expr_as_double(operand);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => format!("(double){n}"),
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            if !d.contains('.') {
                let (scaled, _) = parse_decimal_literal(d);
                scaled.to_string()
            } else {
                d.to_string()
            }
        }
        _ => {
            let e = emit_expr_as_numeric(expr);
            format!("(double)({e})")
        }
    }
}

/// Emit alphanumeric MAX/MIN: builds arrays of pointers and lengths, calls runtime,
/// returns pointer to the winning element.
pub(crate) fn emit_alpha_max_min(args: &[HirExpr], func: &str) -> String {
    let n = args.len();
    let mut ptrs = Vec::new();
    let mut lens = Vec::new();
    for arg in args {
        let (c_src, c_len) = emit_string_func_arg(arg);
        ptrs.push(format!("(const uint8_t*){c_src}"));
        lens.push(c_len);
    }
    let ptrs_init = ptrs.join(", ");
    let lens_init = lens.join(", ");
    format!(
        "({{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
         uint32_t _al[] = {{{lens_init}}}; \
         int32_t _ai = {func}(_ap, _al, {n}); \
         _ap[_ai]; }})"
    )
}

fn emit_comm_mode(mode: &cobol_hir::HirCommunicationMode) -> i32 {
    match mode {
        cobol_hir::HirCommunicationMode::Input => 0,
        cobol_hir::HirCommunicationMode::Output => 1,
        cobol_hir::HirCommunicationMode::InputOutput => 2,
    }
}

fn emit_comm_arg(expr: &HirExpr, data_items: &[HirDataItem]) -> (String, String) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            (
                format!("(const uint8_t*)\"{escaped}\""),
                s.len().to_string(),
            )
        }
        HirExpr::DataRef(data_ref) => {
            let c_expr = emit_data_ref_expr(data_ref);
            let item = find_data_item_by_name(&data_ref.name, data_items);
            let ptr = match item.map(|item| &item.data_type) {
                Some(HirType::Alphanumeric { .. } | HirType::National { .. }) => {
                    format!("(const uint8_t*)(const void*){c_expr}")
                }
                _ => format!("(const uint8_t*)(const void*)&({c_expr})"),
            };
            let len = if let Some(refmod) = &data_ref.refmod {
                refmod
                    .length
                    .as_ref()
                    .map(|length| emit_expr_as_numeric(length))
                    .unwrap_or_else(|| {
                        let full_len = item
                            .map(|item| data_item_byte_size(&item.data_type))
                            .unwrap_or_else(|| find_data_item_size(&c_expr, data_items));
                        let start = emit_expr_as_numeric(&refmod.start);
                        format!("(({full_len}) - ({start}) + 1)")
                    })
            } else {
                item.map(|item| data_item_byte_size(&item.data_type))
                    .unwrap_or_else(|| find_data_item_size(&c_expr, data_items))
                    .to_string()
            };
            (ptr, len)
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            let ptr = c_ptr_expr(&c_name, data_items);
            let len = find_data_item_layout(&c_name, data_items)
                .item_len
                .to_string();
            (format!("(const uint8_t*)(const void*){ptr}"), len)
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            let c_expr = emit_subscript_access(variable, subscripts);
            let item = find_data_item_by_name(variable, data_items);
            let ptr = match item.map(|item| &item.data_type) {
                Some(HirType::Alphanumeric { .. } | HirType::National { .. }) => {
                    format!("(const uint8_t*)(const void*){c_expr}")
                }
                _ => format!("(const uint8_t*)(const void*)&({c_expr})"),
            };
            let len = item
                .map(|item| data_item_byte_size(&item.data_type))
                .unwrap_or_else(|| {
                    expr_data_name(expr)
                        .map(data_name_to_c_name)
                        .map(|name| find_data_item_layout(&name, data_items).item_len)
                        .unwrap_or(0)
                })
                .to_string();
            (ptr, len)
        }
        _ => {
            let c_expr = emit_expr(expr);
            (
                format!("(const uint8_t*)(const void*)&(int64_t){{ {c_expr} }}"),
                "sizeof(int64_t)".to_string(),
            )
        }
    }
}

fn emit_comm_status_updates(
    out: &mut String,
    c_target: &str,
    rc_expr: &str,
    text_len_expr: Option<&str>,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let Some(binding) = with_active_context(|ctx| ctx.communication_binding(c_target)) else {
        return;
    };

    if let Some(status_key) = binding.status_key {
        out.push_str(&format!(
            "{pad}cobol_move_string((const uint8_t*)((({rc_expr}) == 0) ? \"00\" : (({rc_expr}) == 10 ? \"10\" : (({rc_expr}) == 20 ? \"20\" : (({rc_expr}) == 21 ? \"21\" : (({rc_expr}) == 30 ? \"30\" : (({rc_expr}) == 40 ? \"40\" : (({rc_expr}) == 50 ? \"50\" : (({rc_expr}) == 60 ? \"60\" : \"99\")))))))), 2, (uint8_t*){}, {});\n",
            c_ptr_expr(&status_key, data_items),
            find_data_item_size(&status_key, data_items)
        ));
    }
    if let Some(message_count) = binding.message_count {
        out.push_str(&format!(
            "{pad}uint32_t _comm_count = cobol_comm_message_count((const uint8_t*)\"{c_target}\", {});\n",
            c_target.len()
        ));
        emit_store_int(out, &message_count, "(int64_t)_comm_count", data_items, pad);
    }
    if let (Some(text_length), Some(text_len_expr)) = (binding.text_length, text_len_expr) {
        if let Some(disp_size) = grp_display_size(&text_length, data_items) {
            out.push_str(&format!(
                "{pad}cobol_store_numeric_display((int64_t){text_len_expr}, {}, {disp_size});\n",
                display_numeric_ptr(&text_length)
            ));
        } else {
            let ptr = c_ptr_expr(&text_length, data_items);
            let len = find_data_item_size(&text_length, data_items);
            out.push_str(&format!(
                "{pad}cobol_store_numeric_display((int64_t){text_len_expr}, (uint8_t*){ptr}, {len});\n"
            ));
        }
    }
    if let Some(end_key) = binding.end_key {
        out.push_str(&format!(
            "{pad}cobol_move_string((const uint8_t*)(({rc_expr}) == 10 ? \"1\" : \"0\"), 1, (uint8_t*){}, {});\n",
            c_ptr_expr(&end_key, data_items),
            find_data_item_size(&end_key, data_items)
        ));
    }
    if let Some(error_key) = binding.error_key {
        out.push_str(&format!(
            "{pad}if (({rc_expr}) != 20) cobol_move_string((const uint8_t*)\"0\", 1, (uint8_t*){}, {});\n",
            c_ptr_expr(&error_key, data_items),
            find_data_item_layout(&error_key, data_items).area_len
        ));
    }
}

#[derive(Default)]
struct CommSelectors {
    queue_ptr: String,
    queue_len: String,
    sub1_ptr: String,
    sub1_len: String,
    sub2_ptr: String,
    sub2_len: String,
    sub3_ptr: String,
    sub3_len: String,
}

fn null_comm_arg() -> (String, String) {
    ("NULL".to_string(), "0".to_string())
}

fn emit_optional_comm_item(name: Option<&str>, data_items: &[HirDataItem]) -> (String, String) {
    name.map(|name| {
        (
            format!("(const uint8_t*){}", c_ptr_expr(name, data_items)),
            find_data_item_layout(name, data_items).item_len.to_string(),
        )
    })
    .unwrap_or_else(null_comm_arg)
}

struct CommAreaLayoutArg {
    ptr: String,
    item_len: String,
    stride: String,
    count: String,
    area_len: String,
}

fn emit_optional_comm_area_layout(
    name: Option<&str>,
    data_items: &[HirDataItem],
) -> CommAreaLayoutArg {
    name.map(|name| {
        let item_len = find_data_item_element_size(name, data_items);
        let count = find_data_item_occurs_count(name, data_items);
        let (stride, area_len) = comm_area_physical_stride_and_area_len(
            name,
            data_items,
            item_len,
            find_data_item_stride(name, data_items),
            count,
            find_data_item_area_size(name, data_items),
        );
        CommAreaLayoutArg {
            ptr: c_ptr_expr(name, data_items),
            item_len: item_len.to_string(),
            stride: stride.to_string(),
            count: count.to_string(),
            area_len: area_len.to_string(),
        }
    })
    .unwrap_or_else(|| CommAreaLayoutArg {
        ptr: "NULL".to_string(),
        item_len: "0".to_string(),
        stride: "0".to_string(),
        count: "0".to_string(),
        area_len: "0".to_string(),
    })
}

fn comm_area_physical_stride_and_area_len(
    name: &str,
    data_items: &[HirDataItem],
    item_len: u32,
    stride: u32,
    count: u32,
    area_len: u32,
) -> (u32, u32) {
    let lookup = extract_leaf_member(name);
    let Some(item) = find_original_data_item_by_sanitized_name(lookup, data_items) else {
        return (stride, area_len);
    };
    if item.occurs.is_some() && matches!(item.data_type, HirType::Alphanumeric { .. }) {
        let physical_stride = item_len + 1;
        return (physical_stride, physical_stride.saturating_mul(count));
    }
    (stride, area_len)
}

fn emit_comm_selectors(
    binding: &CommunicationBinding,
    data_items: &[HirDataItem],
) -> CommSelectors {
    let (queue_ptr, queue_len) =
        emit_optional_comm_item(binding.symbolic_queue.as_deref(), data_items);
    let (sub1_ptr, sub1_len) =
        emit_optional_comm_item(binding.symbolic_sub_queue_1.as_deref(), data_items);
    let (sub2_ptr, sub2_len) =
        emit_optional_comm_item(binding.symbolic_sub_queue_2.as_deref(), data_items);
    let (sub3_ptr, sub3_len) =
        emit_optional_comm_item(binding.symbolic_sub_queue_3.as_deref(), data_items);
    CommSelectors {
        queue_ptr,
        queue_len,
        sub1_ptr,
        sub1_len,
        sub2_ptr,
        sub2_len,
        sub3_ptr,
        sub3_len,
    }
}

fn emit_numeric_expr_for_var(name: &str, data_items: &[HirDataItem]) -> String {
    let size = grp_display_size(name, data_items)
        .or_else(|| with_active_context(|ctx| ctx.display_numeric_size(name)))
        .unwrap_or(0);
    if size > 0 {
        format!(
            "cobol_display_to_int64((const uint8_t*){}, {})",
            display_numeric_ptr(name),
            size
        )
    } else if find_data_item_by_c_name(name, data_items).is_some_and(|item| {
        matches!(
            item.data_type,
            HirType::Numeric { .. } | HirType::Binary { .. } | HirType::Index
        )
    }) {
        name.to_string()
    } else {
        let ptr = c_ptr_expr(name, data_items);
        format!("(*(const int64_t*){ptr})")
    }
}

/// Emit alphanumeric ORD-MAX/ORD-MIN: returns 1-based position.
pub(crate) fn emit_alpha_ord_max_min(args: &[HirExpr], func: &str) -> String {
    let n = args.len();
    let mut ptrs = Vec::new();
    let mut lens = Vec::new();
    for arg in args {
        let (c_src, c_len) = emit_string_func_arg(arg);
        ptrs.push(format!("(const uint8_t*){c_src}"));
        lens.push(c_len);
    }
    let ptrs_init = ptrs.join(", ");
    let lens_init = lens.join(", ");
    format!(
        "({{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
         uint32_t _al[] = {{{lens_init}}}; \
         {func}(_ap, _al, {n}); }})"
    )
}

/// Helper to extract (c_source_ptr, byte_size) for a string function argument.
/// For string literals, returns ("\"escaped\"", len).
/// For variables, returns (c_name, sizeof(c_name)).
/// For nested string functions, returns the expression and its size.
pub(crate) fn emit_string_func_arg(expr: &HirExpr) -> (String, String) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            (format!("\"{escaped}\""), format!("{}", s.len()))
        }
        HirExpr::DataRef(data_ref) => {
            let c_name = emit_data_ref_expr(data_ref);
            let base_name = data_name_to_c_name(&data_ref.name);
            let len = with_active_context(|ctx| {
                ctx.data_item_size(&base_name)
                    .or_else(|| ctx.data_item_size(extract_leaf_member(&c_name)))
                    .unwrap_or_else(|| find_data_item_size(&base_name, &[]))
            });
            (c_name, len.to_string())
        }
        HirExpr::Variable(name) => {
            let c_name = data_name_to_c_name(name);
            let len = with_active_context(|ctx| {
                ctx.data_item_size(&c_name)
                    .unwrap_or_else(|| find_data_item_size(&c_name, &[]))
            });
            (c_name, len.to_string())
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_fn = name.to_uppercase();
            match upper_fn.as_str() {
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    if let Some(inner) = args.first() {
                        let (_, size) = emit_string_func_arg(inner);
                        let e = emit_expr(expr);
                        (e, size)
                    } else {
                        ("((uint8_t*)0)".to_string(), "0".to_string())
                    }
                }
                "CHAR" => (emit_expr(expr), "1".to_string()),
                "CURRENT-DATE" | "WHEN-COMPILED" => (emit_expr(expr), "21".to_string()),
                _ => {
                    let e = emit_expr(expr);
                    (format!("&{e}"), format!("sizeof({e})"))
                }
            }
        }
        _ => {
            let e = emit_expr(expr);
            (e.clone(), format!("sizeof({e})"))
        }
    }
}

/// Find a field's offset and size within a record, using file-format byte sizes.
/// This is like find_field_offset_and_size but uses COBOL file storage sizes.
fn find_sort_field_offset_and_size(
    field_name: &str,
    record_name: &str,
    data_items: &[HirDataItem],
) -> Option<(u32, u32)> {
    let field_c = sanitize_name(field_name);
    let record_c = sanitize_name(record_name);
    for item in data_items {
        if sanitize_name(&item.name) == record_c {
            if let HirType::Group { members, .. } = &item.data_type {
                return find_sort_field_in_group(&field_c, members, 0);
            }
        }
    }
    None
}

fn find_sort_field_in_group(
    field_c: &str,
    members: &[HirDataItem],
    base_offset: u32,
) -> Option<(u32, u32)> {
    let mut offset = base_offset;
    for item in members {
        let item_c = sanitize_name(&item.name);
        let item_size = cobol_file_byte_size(item);
        if item_c == field_c {
            return Some((offset, item_size));
        }
        if let HirType::Group { members: sub, .. } = &item.data_type {
            if let Some(found) = find_sort_field_in_group(field_c, sub, offset) {
                return Some(found);
            }
        }
        offset += item_size;
    }
    None
}

/// Check if an SD record needs serialize/deserialize for SORT buffer operations.
/// Returns true if the record has fields whose C struct representation differs
/// from the COBOL display format (CobolDecimal or Binary with potential padding).
fn sort_record_needs_conversion(record_name: &str, data_items: &[HirDataItem]) -> bool {
    fn check_members(members: &[HirDataItem]) -> bool {
        for item in members {
            match &item.data_type {
                HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => return true,
                HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => return true,
                HirType::Binary { .. } => return true,
                HirType::Group { members: sub, .. } => {
                    if check_members(sub) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    let c_name = sanitize_name(record_name);
    if let Some(item) = find_data_item_by_c_name(&c_name, data_items) {
        if let HirType::Group { members, .. } = &item.data_type {
            return check_members(members);
        }
    }
    false
}

fn sort_file_runtime_org(
    ctx: &CodegenContext,
    file_name: &str,
    record_name: &str,
    data_items: &[HirDataItem],
) -> u32 {
    let org = ctx.file_organization(file_name).unwrap_or(1);
    if org == 1 && sort_record_needs_conversion(record_name, data_items) {
        0
    } else {
        org
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_sort_procedure_call(
    out: &mut String,
    proc_name: &str,
    thru_name: Option<&str>,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
    preserve_existing_debug_event: bool,
) {
    let Some(target) = transfer_target_for_paragraph_name(proc_name, paragraphs) else {
        let pad = "    ".repeat(indent);
        let c_proc = sanitize_name(proc_name);
        out.push_str(&format!("{pad}para_{c_proc}();\n"));
        return;
    };
    let pad = "    ".repeat(indent);
    if preserve_existing_debug_event {
        out.push_str(&format!("{pad}_preserve_debug_event_once = 1;\n"));
    }
    let through = thru_name.and_then(|name| transfer_target_for_paragraph_name(name, paragraphs));
    let kind = HirPerformKind::ProcedureName { target, through };
    emit_perform(
        out,
        &kind,
        data_items,
        paragraphs,
        fs_map,
        has_declaratives,
        None,
        indent,
    );
    if preserve_existing_debug_event {
        out.push_str(&format!("{pad}_preserve_debug_event_once = 0;\n"));
    }
}

fn transfer_target_for_paragraph_name(
    name: &str,
    paragraphs: &[HirParagraph],
) -> Option<HirTransferTarget> {
    let c_name = sanitize_name(name);
    paragraphs
        .iter()
        .find(|paragraph| sanitize_name(&paragraph.name) == c_name)
        .map(|paragraph| HirTransferTarget::Paragraph {
            id: paragraph.id,
            name: paragraph.name.clone(),
        })
}

/// Emit code to deserialize display-format bytes from a flat buffer into the SD
/// record struct. Handles CobolDecimal and Binary fields correctly.
fn emit_sort_record_deserialize(
    out: &mut String,
    record_var: &str,
    data_items: &[HirDataItem],
    flat_var: &str,
    pad: &str,
) {
    let c_rec = sanitize_name(record_var);
    if let Some(item) = find_data_item_by_c_name(&c_rec, data_items) {
        if let HirType::Group { members, .. } = &item.data_type {
            emit_field_deserialize(out, &c_rec, members, flat_var, pad, 0);
        }
    }
}

/// Emit code to convert CobolDecimal fields from display format to int64_t
/// binary in-place within the sort buffer. Used when USING reads display-format
/// records but the sort comparator expects binary values for CobolDecimal fields.
fn emit_sort_buf_display_to_binary(
    out: &mut String,
    record_name: &str,
    data_items: &[HirDataItem],
    rec_len: u32,
    buf_name: &str,
    count_name: &str,
    pad: &str,
) {
    let c_rec = sanitize_name(record_name);
    let members = data_items.iter().find_map(|item| {
        if sanitize_name(&item.name) == c_rec {
            if let HirType::Group { members, .. } = &item.data_type {
                return Some(members.as_slice());
            }
        }
        None
    });
    let members = match members {
        Some(m) => m,
        None => return,
    };
    out.push_str(&format!(
        "{pad}for (uint32_t _si = 0; _si < {count_name}; _si++) {{\n"
    ));
    out.push_str(&format!(
        "{pad}    uint8_t* _rec = &{buf_name}[_si * {rec_len}];\n"
    ));
    out.push_str(&format!("{pad}    CobolDecimal _tmp_dec;\n"));
    let mut offset: u32 = 0;
    let mut name_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for member in members {
        let _c_name = dedup_member_name(&member.name, &mut name_counts);
        let size = cobol_file_byte_size(member);
        match &member.data_type {
            HirType::Numeric { decimal_places, .. }
                if *decimal_places > 0
                    && display_numeric_c_expr_info(&_c_name, data_items).is_none() =>
            {
                out.push_str(&format!(
                    "{pad}    cobol_decimal_from_string(_rec + {offset}, {size}, &_tmp_dec);\n"
                ));
                out.push_str(&format!("{pad}    memset(_rec + {offset}, 0, {size});\n"));
                out.push_str(&format!(
                    "{pad}    memcpy(_rec + {offset}, &_tmp_dec.value, sizeof(int64_t));\n"
                ));
            }
            HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
                out.push_str(&format!(
                    "{pad}    cobol_decimal_from_string(_rec + {offset}, {size}, &_tmp_dec);\n"
                ));
                out.push_str(&format!("{pad}    memset(_rec + {offset}, 0, {size});\n"));
                out.push_str(&format!(
                    "{pad}    memcpy(_rec + {offset}, &_tmp_dec.value, sizeof(int64_t));\n"
                ));
            }
            _ => {}
        }
        offset += size;
    }
    out.push_str(&format!("{pad}}}\n"));
}

/// Compute a deduplicated C member name, matching the data codegen convention.
fn dedup_member_name(
    base_name: &str,
    counts: &mut std::collections::HashMap<String, u32>,
) -> String {
    let c = sanitize_name(base_name);
    let count = counts.entry(c.clone()).or_insert(0);
    *count += 1;
    if *count > 1 {
        format!("{}_{}", c, count)
    } else {
        c
    }
}

/// Get the COBOL file/record byte size for a data item.
/// For Binary (COMP), computes the actual storage byte count from digit count.
/// For DISPLAY numeric with SIGN SEPARATE, includes the separate sign byte.
fn cobol_file_byte_size(item: &HirDataItem) -> u32 {
    match &item.data_type {
        HirType::Binary { size } => {
            // COMP: digits → bytes per COBOL standard
            if *size <= 4 {
                2
            } else if *size <= 9 {
                4
            } else {
                8
            }
        }
        _ => data_item_storage_size(item),
    }
}

fn emit_field_deserialize(
    out: &mut String,
    record_var: &str,
    members: &[HirDataItem],
    flat_var: &str,
    pad: &str,
    base_offset: u32,
) {
    let mut offset = base_offset;
    let mut name_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for member in members {
        if member.redefines.is_some() || member.renames.is_some() {
            continue;
        }
        let c_name = dedup_member_name(&member.name, &mut name_counts);
        let size = cobol_file_byte_size(member);
        let member_expr = format!("{record_var}.members._m_{c_name}");
        match &member.data_type {
            HirType::Numeric { decimal_places, .. }
                if *decimal_places > 0
                    && display_numeric_c_expr_info(&member_expr, &[]).is_some() =>
            {
                out.push_str(&format!(
                    "{pad}memcpy({member_expr}, {flat_var} + {offset}, {size});\n"
                ));
            }
            HirType::Numeric {
                decimal_places,
                is_signed,
                ..
            } if *decimal_places > 0 => {
                // Restore CobolDecimal value from int64_t binary
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.value = 0;\n"
                ));
                out.push_str(&format!(
                    "{pad}memcpy(&{record_var}.members._m_{c_name}.value, \
                     {flat_var} + {offset}, sizeof(int64_t));\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.scale = {decimal_places};\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.size = {size};\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.is_signed = {};\n",
                    i32::from(*is_signed)
                ));
            }
            HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
                // Restore CobolDecimal value from int64_t binary
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.value = 0;\n"
                ));
                out.push_str(&format!(
                    "{pad}memcpy(&{record_var}.members._m_{c_name}.value, \
                     {flat_var} + {offset}, sizeof(int64_t));\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.scale = {decimal_places};\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.size = {size};\n"
                ));
                out.push_str(&format!(
                    "{pad}{record_var}.members._m_{c_name}.is_signed = 1;\n"
                ));
            }
            HirType::Binary { .. } => {
                // Binary fields are stored as int64_t in C but occupy fewer bytes in
                // the file. Read from flat buffer, sign-extend to int64_t.
                let bsize = size; // use cobol_file_byte_size result
                out.push_str(&format!("{pad}{record_var}.members._m_{c_name} = 0;\n"));
                out.push_str(&format!(
                    "{pad}memcpy(&{record_var}.members._m_{c_name}, \
                     {flat_var} + {offset}, {bsize});\n"
                ));
                // Sign-extend for signed binary
                if bsize < 8 {
                    let bits = bsize * 8;
                    out.push_str(&format!(
                        "{pad}if ({record_var}.members._m_{c_name} & ((int64_t)1 << {msb})) \
                         {record_var}.members._m_{c_name} |= ~(((int64_t)1 << {bits}) - 1);\n",
                        msb = bits - 1,
                        bits = bits,
                    ));
                }
            }
            HirType::Group {
                members: sub,
                size: gsize,
                ..
            } => {
                let has_complex = sub.iter().any(|m| {
                    matches!(&m.data_type,
                        HirType::Numeric { decimal_places, .. } if *decimal_places > 0)
                        || matches!(&m.data_type,
                            HirType::Comp3 { decimal_places, .. } if *decimal_places > 0)
                        || matches!(&m.data_type, HirType::Binary { .. })
                });
                if has_complex {
                    let nested_path = format!("{record_var}.members._m_{c_name}");
                    emit_field_deserialize(out, &nested_path, sub, flat_var, pad, offset);
                } else {
                    out.push_str(&format!(
                        "{pad}memcpy(&{record_var}.members._m_{c_name}, \
                         {flat_var} + {offset}, {gsize});\n"
                    ));
                }
            }
            _ => {
                let destination = sort_record_member_memcpy_pointer(record_var, &c_name, member);
                out.push_str(&format!(
                    "{pad}memcpy({destination}, \
                     {flat_var} + {offset}, {size});\n"
                ));
            }
        }
        offset += size;
    }
}

/// Emit code to serialize the SD record struct to display-format bytes in a flat buffer.
fn emit_sort_record_serialize(
    out: &mut String,
    record_var: &str,
    data_items: &[HirDataItem],
    flat_var: &str,
    pad: &str,
) {
    let c_rec = sanitize_name(record_var);
    if let Some(item) = find_data_item_by_c_name(&c_rec, data_items) {
        if let HirType::Group { members, .. } = &item.data_type {
            emit_field_serialize(out, &c_rec, members, flat_var, pad, 0);
        }
    }
}

fn emit_field_serialize(
    out: &mut String,
    record_var: &str,
    members: &[HirDataItem],
    flat_var: &str,
    pad: &str,
    base_offset: u32,
) {
    let mut offset = base_offset;
    let mut name_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for member in members {
        if member.redefines.is_some() || member.renames.is_some() {
            continue;
        }
        let c_name = dedup_member_name(&member.name, &mut name_counts);
        let size = cobol_file_byte_size(member);
        let member_expr = format!("{record_var}.members._m_{c_name}");
        match &member.data_type {
            HirType::Numeric { decimal_places, .. }
                if *decimal_places > 0
                    && display_numeric_c_expr_info(&member_expr, &[]).is_some() =>
            {
                out.push_str(&format!(
                    "{pad}memcpy({flat_var} + {offset}, {member_expr}, {size});\n"
                ));
            }
            HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
                // Store CobolDecimal value as int64_t binary for correct
                // signed comparison during sorting.
                out.push_str(&format!("{pad}memset({flat_var} + {offset}, 0, {size});\n"));
                out.push_str(&format!(
                    "{pad}memcpy({flat_var} + {offset}, \
                     &{record_var}.members._m_{c_name}.value, sizeof(int64_t));\n"
                ));
            }
            HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
                // Store CobolDecimal value as int64_t binary for correct
                // signed comparison during sorting.
                out.push_str(&format!("{pad}memset({flat_var} + {offset}, 0, {size});\n"));
                out.push_str(&format!(
                    "{pad}memcpy({flat_var} + {offset}, \
                     &{record_var}.members._m_{c_name}.value, sizeof(int64_t));\n"
                ));
            }
            HirType::Binary { .. } => {
                out.push_str(&format!(
                    "{pad}memcpy({flat_var} + {offset}, \
                     &{record_var}.members._m_{c_name}, {size});\n"
                ));
            }
            HirType::Group {
                members: sub,
                size: gsize,
                ..
            } => {
                let has_complex = sub.iter().any(|m| {
                    matches!(&m.data_type,
                        HirType::Numeric { decimal_places, .. } if *decimal_places > 0)
                        || matches!(&m.data_type,
                            HirType::Comp3 { decimal_places, .. } if *decimal_places > 0)
                        || matches!(&m.data_type, HirType::Binary { .. })
                });
                if has_complex {
                    let nested_path = format!("{record_var}.members._m_{c_name}");
                    emit_field_serialize(out, &nested_path, sub, flat_var, pad, offset);
                } else {
                    out.push_str(&format!(
                        "{pad}memcpy({flat_var} + {offset}, \
                         &{record_var}.members._m_{c_name}, {gsize});\n"
                    ));
                }
            }
            _ => {
                let source = sort_record_member_memcpy_pointer(record_var, &c_name, member);
                out.push_str(&format!(
                    "{pad}memcpy({flat_var} + {offset}, {source}, {size});\n"
                ));
            }
        }
        offset += size;
    }
}

fn sort_record_member_memcpy_pointer(
    record_var: &str,
    c_name: &str,
    member: &HirDataItem,
) -> String {
    let member_expr = format!("{record_var}.members._m_{c_name}");
    match &member.data_type {
        HirType::Alphanumeric { .. } | HirType::National { .. } => member_expr,
        _ => format!("&{member_expr}"),
    }
}

fn relative_key_integer_digits(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    find_data_item_by_c_name(c_name, data_items)
        .or_else(|| find_data_item(c_name, data_items))
        .map(|item| match &item.data_type {
            HirType::Numeric {
                size,
                decimal_places,
                ..
            }
            | HirType::Comp3 {
                size,
                decimal_places,
            } => size.saturating_sub(*decimal_places).max(1),
            HirType::Binary { size } => *size,
            _ => find_data_item_size(c_name, data_items),
        })
        .unwrap_or_else(|| find_data_item_size(c_name, data_items))
}

fn relative_key_max_value(digits: u32) -> u64 {
    let capped_digits = digits.min(18);
    10_u64.pow(capped_digits).saturating_sub(1)
}
