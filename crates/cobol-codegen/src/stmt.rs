use super::*;
use cobol_hir::HirPerformTest;

pub(crate) struct StmtEmitEnv<'a> {
    pub(crate) data_items: &'a [HirDataItem],
    pub(crate) paragraphs: &'a [HirParagraph],
    pub(crate) fs_map: &'a FileStatusMap,
    pub(crate) has_declaratives: bool,
    pub(crate) ctx: &'a CodegenContext,
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
        HirStatement::Move { from, to, .. } => {
            for target in to {
                match target {
                    HirMoveTarget::Variable(name) => {
                        let c_target = sanitize_name(name);
                        emit_move_to(out, from, name, &c_target, data_items, &pad);
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
                    }
                }
            }
        }
        HirStatement::MoveCorresponding { from, to, .. } => {
            emit_corresponding_move(out, from, to, data_items, &pad);
        }
        HirStatement::AddCorresponding {
            from,
            to,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(out, from, to, "+", data_items, &pad);
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
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(out, from, to, "-", data_items, &pad);
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
            expr,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            for target in targets {
                let c_target = emit_expr(target);
                let target_name = match target {
                    HirExpr::Variable(name) => name.as_str(),
                    HirExpr::Subscript { variable, .. } => variable.as_str(),
                    _ => "",
                };
                let target_is_decimal = find_data_item(target_name, data_items)
                    .is_some_and(|i| needs_decimal(&i.data_type));
                if has_size_error {
                    let c_expr = emit_int_compatible_expr(expr, data_items);
                    emit_save_and_check_overflow(
                        out,
                        target_name,
                        &c_target,
                        &c_expr,
                        data_items,
                        &pad,
                    );
                } else if target_is_decimal {
                    emit_assign_to_decimal(out, expr, &c_target, data_items, &pad);
                } else {
                    let c_expr = emit_int_compatible_expr(expr, data_items);
                    if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                        let c_target_ptr = display_numeric_ptr(&c_target);
                        out.push_str(&format!(
                            "{pad}cobol_store_numeric_display({c_expr}, \
                             {c_target_ptr}, {disp_size});\n"
                        ));
                    } else {
                        out.push_str(&format!("{pad}{c_target} = {c_expr};\n"));
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
        HirStatement::Add {
            operands,
            to,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
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
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        // For decimal GIVING, build a temp sum then assign
                        out.push_str(&format!("{pad}/* ADD GIVING decimal */\n"));
                        // Use first two addends as decimal add, then chain
                        emit_decimal_giving_add(out, operands, to, &c_target, data_items, &pad);
                    } else if has_size_error {
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size});\n"
                            ));
                            out.push_str(&format!(
                                "{pad}cobol_store_numeric_display({sum_expr}, \
                                 {c_target_ptr}, {disp_size});\n"
                            ));
                        } else {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} = {sum_expr};\n"));
                        }
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        emit_store_int(out, &c_target, &sum_expr, data_items, &pad);
                    }
                }
            } else {
                for target in to {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
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
                    } else {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}cobol_store_numeric_display(\
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}) + ({sum_expr}), \
                                     {c_target_ptr}, {disp_size});\n"
                                ));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                out.push_str(&format!("{pad}{c_target} += {sum_expr};\n"));
                            }
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            emit_store_int_op(out, &c_target, "+", &sum_expr, data_items, &pad);
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
            giving,
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
                // The FROM value is the minuend
                let from_val = if let Some(f) = from.first() {
                    emit_int_compatible_expr(f, data_items)
                } else {
                    "0".to_string()
                };
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        // SUBTRACT GIVING decimal: result = from - sub
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int(\
                             {from_val} - ({sub_expr}), 0, &{c_target});\n"
                        ));
                    } else if has_size_error {
                        let result_expr = format!("{from_val} - ({sub_expr})");
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size});\n"
                            ));
                            out.push_str(&format!(
                                "{pad}cobol_store_numeric_display({result_expr}, \
                                 {c_target_ptr}, {disp_size});\n"
                            ));
                        } else {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} = {result_expr};\n"));
                        }
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        let result_expr = format!("{from_val} - ({sub_expr})");
                        emit_store_int(out, &c_target, &result_expr, data_items, &pad);
                    }
                }
            } else {
                for target in from {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        for op in operands {
                            emit_decimal_arith(
                                out,
                                &c_target,
                                op,
                                "cobol_decimal_sub",
                                data_items,
                                &pad,
                            );
                        }
                    } else {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}cobol_store_numeric_display(\
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}) - ({sum_expr}), \
                                     {c_target_ptr}, {disp_size});\n"
                                ));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                out.push_str(&format!("{pad}{c_target} -= ({sum_expr});\n"));
                            }
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
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
                indent,
            );
        }
        HirStatement::Multiply {
            operand,
            by,
            giving,
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
                let op_is_dec = is_decimal_expr(operand, data_items);
                let by_is_dec = by.first().is_some_and(|b| is_decimal_expr(b, data_items));
                let any_src_decimal = op_is_dec || by_is_dec;
                // For decimal operands, get raw expr (struct); for non-decimal, get int-compatible
                let c_operand_raw = emit_expr(operand);
                let c_operand_int = emit_int_compatible_expr(operand, data_items);
                let first_by_raw = by.first().map(emit_expr).unwrap_or_default();
                let first_by_int = by
                    .first()
                    .map(|b| emit_int_compatible_expr(b, data_items))
                    .unwrap_or_default();
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal || any_src_decimal {
                        if target_is_decimal
                            && emit_fast_decimal_multiply_giving(
                                out,
                                &c_target,
                                target,
                                operand,
                                by.first(),
                                data_items,
                                &pad,
                            )
                        {
                            continue;
                        }
                        // Decimal path: convert operands to CobolDecimal
                        let init_a = if op_is_dec {
                            format!("CobolDecimal _ma = {c_operand_raw};")
                        } else {
                            format!(
                                "CobolDecimal _ma; cobol_decimal_from_int({c_operand_int}, 0, &_ma);"
                            )
                        };
                        let init_b = if by_is_dec {
                            format!("CobolDecimal _mb = {first_by_raw};")
                        } else {
                            format!(
                                "CobolDecimal _mb; cobol_decimal_from_int({first_by_int}, 0, &_mb);"
                            )
                        };
                        out.push_str(&format!("{pad}{{ {init_a} {init_b} "));
                        out.push_str("CobolDecimal _mr; cobol_decimal_mul(&_ma, &_mb, &_mr); ");
                        if target_is_decimal {
                            out.push_str(&format!("{c_target} = _mr; }}\n"));
                        } else if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "cobol_store_numeric_display(\
                                 cobol_decimal_to_int64(&_mr), \
                                 {c_target_ptr}, {disp_size}); }}\n"
                            ));
                        } else {
                            out.push_str(&format!(
                                "{c_target} = cobol_decimal_to_int64(&_mr); }}\n"
                            ));
                        }
                    } else {
                        let mul_expr = format!("{first_by_int} * {c_operand_int}");
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}cobol_store_numeric_display({mul_expr}, \
                                     {c_target_ptr}, {disp_size});\n"
                                ));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                out.push_str(&format!("{pad}{c_target} = {mul_expr};\n"));
                            }
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            emit_store_int(out, &c_target, &mul_expr, data_items, &pad);
                        }
                    }
                }
            } else {
                for target in by {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        emit_decimal_arith(
                            out,
                            &c_target,
                            operand,
                            "cobol_decimal_mul",
                            data_items,
                            &pad,
                        );
                    } else if is_decimal_expr(operand, data_items) {
                        // int64 target *= CobolDecimal operand: use decimal path
                        let c_operand = emit_expr(operand);
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ CobolDecimal _td; cobol_decimal_from_int(\
                                 cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size}), 0, &_td); \
                                 cobol_decimal_mul(&_td, &{c_operand}, &_td); \
                                 cobol_store_numeric_display(\
                                 cobol_decimal_to_int64(&_td), \
                                 {c_target_ptr}, {disp_size}); }}\n"
                            ));
                        } else {
                            out.push_str(&format!(
                                "{pad}{{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                                 cobol_decimal_mul(&_td, &{c_operand}, &_td); \
                                 {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                            ));
                        }
                    } else {
                        let c_operand = emit_int_compatible_expr(operand, data_items);
                        if has_size_error {
                            if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                                let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                                let c_target_ptr = display_numeric_ptr(&c_target);
                                out.push_str(&format!(
                                    "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size});\n"
                                ));
                                out.push_str(&format!(
                                    "{pad}cobol_store_numeric_display(\
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}) * ({c_operand}), \
                                     {c_target_ptr}, {disp_size});\n"
                                ));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                out.push_str(&format!("{pad}{c_target} *= {c_operand};\n"));
                            }
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
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
            giving,
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
                let first_into = into.first().map(emit_expr).unwrap_or_default();
                let first_into_int = into
                    .first()
                    .map(|i| emit_int_compatible_expr(i, data_items))
                    .unwrap_or_default();
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    if let Some(rem) = remainder {
                        let c_rem = emit_expr(rem);
                        let rem_name = expr_var_name(rem);
                        let rem_expr = format!("{first_into_int} % {c_operand_int}");
                        emit_store_int(out, &c_rem, &rem_expr, data_items, &pad);
                        let _ = rem_name; // suppress unused warning
                    }
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal && any_src_decimal {
                        // Use decimal division
                        let init_a = if into_is_dec {
                            format!("CobolDecimal _da = {first_into};")
                        } else {
                            format!(
                                "CobolDecimal _da; cobol_decimal_from_int({first_into_int}, 0, &_da);"
                            )
                        };
                        let init_b = if op_is_dec {
                            format!("CobolDecimal _db = {c_operand};")
                        } else {
                            format!(
                                "CobolDecimal _db; cobol_decimal_from_int({c_operand_int}, 0, &_db);"
                            )
                        };
                        out.push_str(&format!(
                            "{pad}{{ {init_a} {init_b} cobol_decimal_div(&_da, &_db, &{c_target}); }}\n"
                        ));
                    } else if target_is_decimal {
                        out.push_str(&format!(
                            "{pad}if ({c_operand_int} != 0) {{ \
                             cobol_decimal_from_int(\
                             {first_into_int} / {c_operand_int}, 0, &{c_target}); }}\n"
                        ));
                    } else if any_src_decimal {
                        let div_expr = format!("{first_into_int} / {c_operand_int}");
                        emit_store_int(out, &c_target, &div_expr, data_items, &pad);
                    } else if has_size_error {
                        let div_expr = format!("{first_into_int} / {c_operand_int}");
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ int64_t _prev = cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size});\n"
                            ));
                            out.push_str(&format!(
                                "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                                 else {{ cobol_store_numeric_display({div_expr}, \
                                 {c_target_ptr}, {disp_size}); }}\n"
                            ));
                        } else {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!(
                                "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                                 else {{ {c_target} = {div_expr}; }}\n"
                            ));
                        }
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        let div_expr = format!("{first_into_int} / {c_operand_int}");
                        emit_store_int(out, &c_target, &div_expr, data_items, &pad);
                    }
                }
            } else {
                for target in into {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        emit_decimal_arith(
                            out,
                            &c_target,
                            operand,
                            "cobol_decimal_div",
                            data_items,
                            &pad,
                        );
                    } else if is_decimal_expr(operand, data_items) {
                        // int64 target /= CobolDecimal operand
                        if let Some(disp_size) = grp_display_size(&c_target, data_items) {
                            let c_target_const_ptr = display_numeric_const_ptr(&c_target);
                            let c_target_ptr = display_numeric_ptr(&c_target);
                            out.push_str(&format!(
                                "{pad}{{ CobolDecimal _td; cobol_decimal_from_int(\
                                 cobol_display_to_int64(\
                                 {c_target_const_ptr}, {disp_size}), 0, &_td); \
                                 cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                 cobol_store_numeric_display(\
                                 cobol_decimal_to_int64(&_td), \
                                 {c_target_ptr}, {disp_size}); }}\n"
                            ));
                        } else {
                            out.push_str(&format!(
                                "{pad}{{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                                 cobol_decimal_div(&_td, &{c_operand}, &_td); \
                                 {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                            ));
                        }
                    } else {
                        if let Some(rem) = remainder {
                            let c_rem = emit_expr(rem);
                            let rem_name = expr_var_name(rem);
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
                            let _ = rem_name;
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
                                     cobol_display_to_int64(\
                                     {c_target_const_ptr}, {disp_size}) / {c_operand_int}, \
                                     {c_target_ptr}, {disp_size}); }}\n"
                                ));
                            } else {
                                out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                                out.push_str(&format!(
                                    "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                                     else {{ {c_target} /= {c_operand_int}; }}\n"
                                ));
                            }
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
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
            // Extract the program name from the expression.
            // Distinguish static CALL (literal string) from dynamic CALL (variable).
            let (prog_name, is_dynamic) = match program {
                HirExpr::Literal(HirLiteral::String(s)) => (sanitize_name(s), false),
                HirExpr::Variable(name) => {
                    // Check if the variable is a data item (dynamic CALL) or
                    // could be a literal-like reference.
                    let sname = sanitize_name(name);
                    if find_data_item(name, data_items).is_some() {
                        (sname, true)
                    } else {
                        (sname, false)
                    }
                }
                _ => (emit_expr(program), false),
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
                // The variable contains the program name as a string.
                let param_count = params.len();
                if param_count == 0 {
                    out.push_str(&format!("{inner_pad}{{\n"));
                    out.push_str(&format!(
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({prog_name}, sizeof({prog_name}), _name, sizeof(_name));\n"
                    ));
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)(void) = (void(*)(void))dlsym(RTLD_DEFAULT, _name);\n"
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
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({prog_name}, sizeof({prog_name}), _name, sizeof(_name));\n"
                    ));
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)({types_str}) = (void(*)({types_str}))dlsym(RTLD_DEFAULT, _name);\n"
                    ));
                    if has_exception_handlers {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }}\n"
                        ));
                    }
                    out.push_str(&format!("{inner_pad}}}\n"));
                }
            } else if params.is_empty() {
                if has_exception_handlers {
                    // Use file-scope weak declaration for null check
                    out.push_str(&format!("{inner_pad}if ({prog_name}) {{\n"));
                    out.push_str(&format!(
                        "{inner_pad}    jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}(); cobol_call_leave(); }}\n"
                    ));
                    out.push_str(&format!("{inner_pad}}} else {{ _call_failed = 1; }}\n"));
                } else {
                    // Call via file-scope weak declaration — null-check
                    // to gracefully handle missing sub-programs.
                    out.push_str(&format!(
                        "{inner_pad}if ({prog_name}) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}(); cobol_call_leave(); }} }}\n"
                    ));
                }
            } else {
                // Wrap in a block to scope _content_copy_* variables
                // and avoid redefinition when multiple CALLs in same scope.
                out.push_str(&format!("{inner_pad}{{\n"));
                let call_pad = format!("{inner_pad}    ");
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
                let _types_str = param_types.join(", ");
                let values_str = param_values.join(", ");
                if has_exception_handlers {
                    // Use file-scope weak declaration for null check
                    out.push_str(&format!("{call_pad}if ({prog_name}) {{\n"));
                    out.push_str(&format!(
                        "{call_pad}    jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}({values_str}); cobol_call_leave(); }}\n"
                    ));
                    out.push_str(&format!("{call_pad}}} else {{ _call_failed = 1; }}\n"));
                } else {
                    // Call via file-scope weak declaration — null-check
                    // to gracefully handle missing sub-programs.
                    out.push_str(&format!(
                        "{call_pad}if ({prog_name}) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}({values_str}); cobol_call_leave(); }} }}\n"
                    ));
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
                let file_path_str = if entry.assign_to.is_empty() {
                    entry.file_name.as_str()
                } else {
                    entry.assign_to.as_str()
                };
                let escaped_name = escape_c_string(file_path_str);
                let name_len = file_path_str.len();
                let org_val = entry.organization;
                out.push_str(&format!("{pad}/* OPEN {mode_comment} {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, 0, {mode_val}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, 0, {mode_val}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Close { files, .. } => {
            for file in files {
                let c_name = sanitize_name(file);
                out.push_str(&format!("{pad}/* CLOSE {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_close(FILE_ID_{c_name});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}cobol_file_close(FILE_ID_{c_name});\n"));
                }
            }
        }
        HirStatement::Read {
            file_name,
            into,
            at_end,
            not_at_end,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            // Determine the target buffer: INTO variable if specified, else
            // look up the FD record name for this file, falling back to the
            // file name itself.
            let (target, target_name) = if let Some((into_var, into_subs)) = into {
                if into_subs.is_empty() {
                    let n = sanitize_name(into_var);
                    (n.clone(), n)
                } else {
                    let access = emit_subscript_access(into_var, into_subs);
                    let n = sanitize_name(into_var);
                    (access, n)
                }
            } else {
                let r = resolve_file_record(&c_name);
                (r.clone(), r)
            };
            let rec_len = find_record_len(&target_name, data_items);
            out.push_str(&format!("{pad}/* READ {c_name} */\n"));
            out.push_str(&format!(
                "{pad}{{\n{pad}    uint32_t _fs = cobol_file_read_next(FILE_ID_{c_name}, (uint8_t*)&{target}, {rec_len});\n"
            ));
            emit_file_status_update(
                out,
                &c_name,
                "_fs",
                fs_map,
                has_declaratives,
                &format!("{pad}    "),
            );
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
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                c_name.clone()
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* WRITE {c_name} */\n"));
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            if needs_rc {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!(
                    "{pad}    uint32_t _wrc = cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                ));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_wrc",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                }
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
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Rewrite {
            record_name,
            file_name,
            from,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                c_name.clone()
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* REWRITE {c_name} */\n"));
            {
                let has_fs = fs_map.contains_key(&c_file);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Delete { file_name, .. } => {
            let c_name = sanitize_name(file_name);
            out.push_str(&format!("{pad}/* DELETE {c_name} */\n"));
            {
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_delete(FILE_ID_{c_name});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}cobol_file_delete(FILE_ID_{c_name});\n"));
                }
            }
        }
        HirStatement::GoTo {
            targets,
            depending_on,
            ..
        } => {
            let in_body = with_active_context(|ctx| ctx.in_body_context());
            if let Some(dep) = depending_on {
                let c_dep = sanitize_name(dep);
                out.push_str(&format!("{pad}switch ((int){c_dep}) {{\n"));
                for (i, target) in targets.iter().enumerate() {
                    let c_target = sanitize_name(target);
                    if in_body {
                        out.push_str(&format!("{pad}    case {}: goto lbl_{c_target};\n", i + 1));
                    } else {
                        let label_id = with_active_context(|ctx| ctx.label_id(&c_target));
                        if let Some(id) = label_id {
                            out.push_str(&format!(
                                "{pad}    case {}: _goto_target = {id}; return;\n",
                                i + 1
                            ));
                        } else {
                            out.push_str(&format!(
                                "{pad}    case {}: para_{c_target}(); return;\n",
                                i + 1
                            ));
                        }
                    }
                }
                out.push_str(&format!("{pad}    default: break;\n"));
                out.push_str(&format!("{pad}}}\n"));
            } else if let Some(target) = targets.first() {
                let c_target = sanitize_name(target);
                if in_body {
                    out.push_str(&format!("{pad}goto lbl_{c_target};\n"));
                } else {
                    let label_id = with_active_context(|ctx| ctx.label_id(&c_target));
                    if let Some(id) = label_id {
                        out.push_str(&format!("{pad}_goto_target = {id}; return;\n"));
                    } else {
                        out.push_str(&format!("{pad}para_{c_target}(); return;\n"));
                    }
                }
            } else {
                // GO TO. (no target) - alterable GO TO without ALTER applied.
                // Fall through to the next statement (no-op).
                out.push_str(&format!("{pad}/* GO TO (no target - alterable) */\n"));
            }
        }
        HirStatement::Initialize { targets, .. } => {
            for target in targets {
                let c_target = sanitize_name(target);
                emit_initialize_field(out, target, &c_target, data_items, &pad);
            }
        }
        HirStatement::Set { targets, value, .. } => {
            for target in targets {
                let target_name = expr_var_name(target);
                let c_target = emit_expr(target);
                let target_item = find_data_item(target_name, data_items);
                let target_is_decimal = target_item.is_some_and(|i| needs_decimal(&i.data_type));
                let target_is_alpha = target_item
                    .is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
                let target_is_group =
                    target_item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
                let c_tgt_base = sanitize_name(target_name);
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
            on_overflow,
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
                "{pad}    struct {{ const uint8_t* ptr; uint32_t len; const uint8_t* delim_ptr; uint32_t delim_len; }} _sources[{src_count}];\n"
            ));
            for i in 0..src_count {
                out.push_str(&format!(
                    "{pad}    _sources[{i}].ptr = (const uint8_t*)_src_ptr_{i}; _sources[{i}].len = _src_len_{i}; _sources[{i}].delim_ptr = _delim_ptr_{i}; _sources[{i}].delim_len = _delim_len_{i};\n"
                ));
            }
            let into_ptr = c_ptr_expr(&c_into, data_items);
            out.push_str(&format!("{pad}    uint32_t _pointer = 1;\n"));
            out.push_str(&format!(
                "{pad}    int32_t _str_rc = cobol_string_concat(_sources, {src_count}, (uint8_t*){into_ptr}, {into_size}, &_pointer);\n"
            ));
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
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::UnstringStmt {
            source,
            delimiters,
            into,
            on_overflow,
            ..
        } => {
            let c_source = sanitize_name(source);
            let src_size = find_data_item_size(&c_source, data_items);
            let targets: Vec<_> = into.iter().map(|s| sanitize_name(s)).collect();
            let tgt_count = targets.len();
            out.push_str(&format!(
                "{pad}/* UNSTRING {c_source} INTO {} */\n",
                targets.join(", ")
            ));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    struct {{ uint8_t* ptr; uint32_t len; uint8_t* delimiter_ptr; uint32_t delimiter_len; uint32_t* count_ptr; }} _targets[{tgt_count}];\n"
            ));
            for (i, tgt) in targets.iter().enumerate() {
                let tgt_size = find_data_item_size(tgt, data_items);
                let tgt_ptr = c_ptr_expr(tgt, data_items);
                out.push_str(&format!(
                    "{pad}    _targets[{i}].ptr = (uint8_t*){tgt_ptr}; _targets[{i}].len = {tgt_size}; _targets[{i}].delimiter_ptr = NULL; _targets[{i}].delimiter_len = 0; _targets[{i}].count_ptr = NULL;\n"
                ));
            }
            out.push_str(&format!(
                "{pad}    uint32_t _pointer = 1; uint32_t _tallying = 0;\n"
            ));
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
                        let c_d = sanitize_name(name);
                        let d_size = find_data_item_size(&c_d, data_items);
                        let d_ptr = c_ptr_expr(&c_d, data_items);
                        (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                    }
                    _ => ("(const uint8_t*)\" \"".to_string(), "1".to_string()),
                }
            } else {
                ("(const uint8_t*)\" \"".to_string(), "1".to_string())
            };
            let src_ptr = c_ptr_expr(&c_source, data_items);
            out.push_str(&format!(
                "{pad}    int32_t _ustr_rc = cobol_unstring((const uint8_t*){src_ptr}, {src_size}, {delim_ptr}, {delim_len}, _targets, {tgt_count}, &_pointer, &_tallying);\n"
            ));
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
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Accept { target, source, .. } => {
            let c_target = sanitize_name(target);
            let size = find_data_item_size(&c_target, data_items);
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
                    if let Some(item) = find_data_item(target, data_items) {
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
                    out.push_str(&format!("{pad}fgets((char*){tgt_ptr}, {size}, stdin);\n"));
                    out.push_str(&format!(
                        "{pad}((char*){tgt_ptr})[strcspn((char*){tgt_ptr}, \"\\n\")] = '\\0';\n"
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
                            out.push_str(&format!("{pad}}}\n"));
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
                            out.push_str(&format!("{pad}}}\n"));
                        }
                    }
                }
                HirAcceptSource::Console => {}
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
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_enable((const uint8_t*)\"{c_target}\", {}, {}, {}, {c_key_ptr}, {c_key_len}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
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
                source.1
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
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_disable((const uint8_t*)\"{c_target}\", {}, {}, {}, {c_key_ptr}, {c_key_len}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
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
                source.1
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
            let (dest_arg, dest_table_count, dest_count_expr, error_key_arg) = binding
                .as_ref()
                .map(|binding| {
                    (
                        emit_optional_comm_item(binding.destination.as_deref(), data_items),
                        binding.destination_table_count.unwrap_or(0),
                        binding
                            .destination_count
                            .as_ref()
                            .map(|name| emit_numeric_expr_for_var(name, data_items))
                            .unwrap_or_else(|| "0".to_string()),
                        emit_optional_comm_item(binding.error_key.as_deref(), data_items),
                    )
                })
                .unwrap_or_else(|| ((null_comm_arg()), 0, "0".to_string(), (null_comm_arg())));
            let (option_kind, option_value) = match with {
                Some(cobol_hir::HirSendOption::Emi) => ("1".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Egi) => ("2".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Esi) => ("3".to_string(), "0".to_string()),
                Some(cobol_hir::HirSendOption::Identifier(expr)) => {
                    ("4".to_string(), emit_expr_as_numeric(expr))
                }
                None => ("0".to_string(), "0".to_string()),
            };
            out.push_str(&format!(
                "{pad}{{ uint32_t _rc = cobol_comm_send((const uint8_t*)\"{c_target}\", {}, {c_from_ptr}, {c_from_len}, {effective_len}, {option_kind}, {option_value}, {}, {}, {}, {}, {}, {}, {});\n",
                c_target.len(),
                if *replacing_line { 1 } else { 0 },
                dest_arg.0,
                dest_arg.1,
                dest_count_expr,
                dest_table_count,
                error_key_arg.0,
                error_key_arg.1
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
        HirStatement::Receive {
            target,
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
                "{pad}{{ uint32_t _text_len = 0; uint32_t _rc = cobol_comm_receive((const uint8_t*)\"{c_target}\", {}, (uint8_t*){into_ptr}, {into_len}, &_text_len, {}, {}, {}, {}, {}, {}, {}, {});\n",
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
            emit_comm_status_updates(
                out,
                &c_target,
                "_rc",
                Some("_text_len"),
                data_items,
                &format!("{pad}    "),
            );
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
            out.push_str(&format!(
                "{pad}    struct {{ uint32_t offset; uint32_t length; uint8_t ascending; uint8_t key_type; }} _sort_keys[{key_count}];\n"
            ));
            if flat_keys.is_empty() {
                out.push_str(&format!(
                    "{pad}    _sort_keys[0].offset = 0; _sort_keys[0].length = {rec_len}; _sort_keys[0].ascending = 1; _sort_keys[0].key_type = 0;\n"
                ));
            } else {
                for (i, (field_name, ascending)) in flat_keys.iter().enumerate() {
                    let asc_val: u8 = if *ascending { 1 } else { 0 };
                    // Try to find the actual offset and size of the sort key field
                    let kt = sort_key_type_for_field(field_name, data_items);
                    if let Some((offset, size)) =
                        find_field_offset_and_size(field_name, &record_var, data_items)
                    {
                        // No size adjustment - use HIR-based sizes matching file I/O
                        out.push_str(&format!(
                            "{pad}    _sort_keys[{i}].offset = {offset}; _sort_keys[{i}].length = {size}; _sort_keys[{i}].ascending = {asc_val}; _sort_keys[{i}].key_type = {kt}; /* {field_name} */\n"
                        ));
                    } else {
                        // Fallback: use field size but offset 0
                        let field_c = sanitize_name(field_name);
                        let field_size = find_data_item_size(&field_c, data_items);
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
                    out.push_str(&format!(
                        "{pad}    /* USING {c_using}: read all records */\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_file_open(FILE_ID_{c_using}, (const uint8_t*)\"{c_using}\", {using_name_len}, 1, 0, 0, {rec_len});\n",
                        using_name_len = c_using.len()
                    ));
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
                    out.push_str(&format!("{pad}    /* INPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!(
                        "{pad}    _sort_buf_id = cobol_sort_buffer_init({rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort(_sort_buf, _sort_count, {rec_len}, _sort_keys, {key_count});\n"
                ));
                if !giving.is_empty() {
                    for g in giving {
                        let c_giving = sanitize_name(g);
                        out.push_str(&format!(
                            "{pad}    /* GIVING {c_giving}: write sorted records */\n"
                        ));
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_giving}, (const uint8_t*)\"{c_giving}\", {giving_name_len}, 1, 0, 2, {rec_len});\n",
                            giving_name_len = c_giving.len()
                        ));
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
                    out.push_str(&format!("{pad}    /* OUTPUT PROCEDURE {c_proc} */\n"));
                    // Copy sorted data into sort buffer for RETURN
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
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
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
                    out.push_str(&format!("{pad}    /* INPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort_buffer_sort(_sort_buf_id, _sort_keys, {key_count});\n"
                ));
                if let Some((proc_name, thru)) = output_procedure {
                    let c_proc = sanitize_name(proc_name);
                    out.push_str(&format!("{pad}    /* OUTPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
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
            let c_target = sanitize_name(target);
            let target_size = find_data_item_size(&c_target, data_items);
            out.push_str(&format!("{pad}/* INSPECT {c_target} */\n"));
            match kind {
                cobol_hir::HirInspectKind::Tallying { tallying } => {
                    emit_inspect_tallying(out, &c_target, target_size, tallying, data_items, &pad);
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
                    emit_inspect_tallying(out, &c_target, target_size, tallying, data_items, &pad);
                    emit_inspect_replacing(
                        out,
                        &c_target,
                        target_size,
                        replacing,
                        data_items,
                        &pad,
                    );
                }
                cobol_hir::HirInspectKind::Converting { from, to } => {
                    let c_from = emit_inspect_operand(out, from, "conv_from", data_items, &pad);
                    let c_to = emit_inspect_operand(out, to, "conv_to", data_items, &pad);
                    let insp_tgt_ptr = c_ptr_expr(&c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_inspect_converting((uint8_t*){insp_tgt_ptr}, {target_size}, {}, {}, {}, {});\n",
                        c_from.0, c_from.1, c_to.0, c_to.1
                    ));
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
            out.push_str(&format!("{pad}exit(0); /* EXIT PROGRAM */\n"));
        }
        HirStatement::ExitParagraph { .. } => {
            out.push_str(&format!("{pad}return; /* EXIT PARAGRAPH */\n"));
        }
        HirStatement::Continue { .. } => {
            out.push_str(&format!("{pad}/* CONTINUE */\n"));
        }
        HirStatement::Label { name } => {
            let c_name = sanitize_name(name);
            let label = format!("lbl_{c_name}");
            let is_new = with_active_context(|ctx| ctx.mark_label_emitted(label.clone()));
            if is_new {
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
                    "{pad}{c_ret} = cobol_invoke(&{c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
                    params.len()
                ));
            } else {
                out.push_str(&format!(
                    "{pad}cobol_invoke(&{c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
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
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_validate(\"{c_target}\"); /* VALIDATE */\n"
            ));
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
                let c_key = sanitize_name(key_name);
                let key_size = find_data_item_size(&c_key, data_items);
                let is_key_group = find_data_item(key_name.as_str(), data_items)
                    .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
                let addr_prefix = if is_key_group { "&" } else { "" };
                format!("cobol_file_start(FILE_ID_{c_name}, (const uint8_t*){addr_prefix}{c_key}, {key_size}, {mode_val})")
            } else {
                format!("cobol_file_start(FILE_ID_{c_name}, NULL, 0, {mode_val})")
            };
            if needs_rc {
                out.push_str(&format!("{pad}    uint32_t _src = {start_call};\n"));
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
            let target = if let Some((into_var, into_subs)) = into {
                if into_subs.is_empty() {
                    sanitize_name(into_var)
                } else {
                    emit_subscript_access(into_var, into_subs)
                }
            } else {
                record_var
            };
            out.push_str(&format!("{pad}/* RETURN {c_name} */\n"));
            out.push_str(&format!(
                "{pad}{{\n{pad}    uint32_t _fs = cobol_sort_buffer_return(_sort_buf_id, (uint8_t*)&{target}, {rec_len});\n"
            ));
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
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let rec_len = find_record_len(&c_name, data_items);
            out.push_str(&format!("{pad}/* MERGE {c_name} */\n"));
            if !using.is_empty() {
                let using_names: Vec<_> = using.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* USING {} */\n", using_names.join(", ")));
            }
            if !giving.is_empty() {
                let giving_names: Vec<_> = giving.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* GIVING {} */\n", giving_names.join(", ")));
            }
            let key_count = if keys.is_empty() { 1 } else { keys.len() };
            let input_count = using.len();
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    uint32_t _merge_inputs[{input_count}];\n"
            ));
            for (i, input_file) in using.iter().enumerate() {
                let c_input = sanitize_name(input_file);
                out.push_str(&format!(
                    "{pad}    _merge_inputs[{i}] = FILE_ID_{c_input};\n"
                ));
            }
            out.push_str(&format!(
                "{pad}    struct {{ uint32_t offset; uint32_t length; uint8_t ascending; }} _merge_keys[{key_count}];\n"
            ));
            if keys.is_empty() {
                out.push_str(&format!(
                    "{pad}    _merge_keys[0].offset = 0; _merge_keys[0].length = {rec_len}; _merge_keys[0].ascending = 1;\n"
                ));
            } else {
                for (i, key) in keys.iter().enumerate() {
                    let ascending = matches!(key.order, cobol_hir::HirSortOrder::Ascending);
                    let asc_val: u8 = if ascending { 1 } else { 0 };
                    out.push_str(&format!(
                        "{pad}    _merge_keys[{i}].offset = 0; _merge_keys[{i}].length = {rec_len}; _merge_keys[{i}].ascending = {asc_val};\n"
                    ));
                }
            }
            let output_file_id = if let Some(first_giving) = giving.first() {
                let c_giving = sanitize_name(first_giving);
                format!("FILE_ID_{c_giving}")
            } else {
                format!("FILE_ID_{c_name}")
            };
            out.push_str(&format!(
                "{pad}    cobol_merge(_merge_inputs, {input_count}, {output_file_id}, _merge_keys, {key_count}, {rec_len});\n"
            ));
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Release {
            record_name, from, ..
        } => {
            let c_name = sanitize_name(record_name);
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* RELEASE {c_name} */\n"));
            out.push_str(&format!(
                "{pad}cobol_sort_buffer_release(_sort_buf_id, (const uint8_t*)&{source}, {rec_len});\n"
            ));
        }
        // --- Table handling: SEARCH ---
        HirStatement::Search {
            table_name,
            all: _,
            varying,
            at_end,
            when_clauses,
            ..
        } => {
            let c_table = sanitize_name(table_name);
            let c_idx = if let Some(ref v) = varying {
                sanitize_name(v)
            } else {
                // Use the first INDEXED BY name from the OCCURS clause
                find_first_index_name(&c_table, data_items)
                    .unwrap_or_else(|| format!("{c_table}_IDX"))
            };
            let max_occurs = find_occurs_count(&c_table, data_items);
            let inner_pad = "    ".repeat(indent + 1);
            let inner2_pad = "    ".repeat(indent + 2);
            out.push_str(&format!("{pad}/* SEARCH {c_table} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!("{inner_pad}int _search_found = 0;\n"));
            out.push_str(&format!(
                "{inner_pad}for (; {c_idx} <= {max_occurs}; {c_idx}++) {{\n"
            ));
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
        // --- Report writer statements (stub — emit comments) ---
        HirStatement::Initiate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* INITIATE {c_name} */\n"));
            }
        }
        HirStatement::Generate { report_name, .. } => {
            let c_name = sanitize_name(report_name);
            out.push_str(&format!("{pad}/* GENERATE {c_name} */\n"));
        }
        HirStatement::Terminate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* TERMINATE {c_name} */\n"));
            }
        }
    }
}

pub(crate) fn emit_display_operand(
    out: &mut String,
    expr: &HirExpr,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match expr {
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
            let c_name = sanitize_name(name);
            let item = find_data_item(name, data_items);

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
            if is_decimal {
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
            } else if let Some(disp_size) = grp_display_size(&c_name, data_items) {
                let c_name_ptr = display_numeric_const_ptr(&c_name);
                out.push_str(&format!(
                    "{pad}cobol_display_int(cobol_display_to_int64(\
                     {c_name_ptr}, {disp_size}));\n"
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
            let c_var = sanitize_name(variable);
            let c_start = emit_expr(start);
            let var_size = find_data_item_size(&c_var, data_items);
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
            let item = find_data_item(variable, data_items);
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
            } else if is_decimal {
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_access}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else {
                let c_var = sanitize_name(variable);
                if let Some(disp_size) = grp_display_size(&c_var, data_items) {
                    let c_access_ptr = display_numeric_const_ptr(&c_access);
                    out.push_str(&format!(
                        "{pad}cobol_display_int(cobol_display_to_int64(\
                         {c_access_ptr}, {disp_size}));\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}cobol_display_int({c_access});\n"));
                }
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
    // Display the SOURCE field if present
    if let Some(ref source) = si.source {
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

pub(crate) fn emit_move_to(
    out: &mut String,
    from: &HirExpr,
    target_name: &smol_str::SmolStr,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_c_name = sanitize_name(target_name);
    let target_type = find_data_item(target_name.as_str(), data_items).map(|item| &item.data_type);
    let inherited_target_alpha = with_active_context(|ctx| ctx.is_group_alpha_name(&target_c_name));
    let inherited_target_group = with_active_context(|ctx| ctx.is_group_name(&target_c_name));
    let is_target_alpha =
        matches!(target_type, Some(HirType::Alphanumeric { .. })) || inherited_target_alpha;
    let is_target_group =
        matches!(target_type, Some(HirType::Group { .. })) || inherited_target_group;
    let is_target_national = matches!(target_type, Some(HirType::National { .. }));
    let is_target_decimal = target_type.is_some_and(needs_decimal)
        || with_active_context(|ctx| ctx.is_decimal_name(&target_c_name));

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
            HirExpr::Variable(src_name) => {
                let c_src = sanitize_name(src_name);
                let src_item = find_data_item(src_name.as_str(), data_items).map(|i| &i.data_type);
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
                    let src_size = find_data_item_size(&c_src, data_items);
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
        if let HirExpr::Variable(src_name) = from {
            let c_src = sanitize_name(src_name);
            let is_source_group = find_data_item(src_name.as_str(), data_items)
                .is_some_and(|item| matches!(item.data_type, HirType::Group { .. }));
            if is_source_group {
                let src_ptr = c_ptr_expr(&c_src, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                let src_size = find_data_item_storage_size(&c_src, data_items);
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}{{\n\
                     {pad}    size_t _src_sz = {src_size};\n\
                     {pad}    size_t _tgt_sz = {tgt_size};\n\
                     {pad}    size_t _cp_sz = _src_sz < _tgt_sz ? _src_sz : _tgt_sz;\n\
                     {pad}    memcpy({tgt_ptr}, {src_ptr}, _cp_sz);\n\
                     {pad}    if (_src_sz < _tgt_sz) {{\n\
                     {pad}        memset((uint8_t*){tgt_ptr} + _src_sz, ' ', \
                     _tgt_sz - _src_sz);\n\
                     {pad}    }}\n\
                     {pad}}}\n"
                ));
            } else {
                // Non-group source to group target: copy by COBOL data size
                let src_size = find_data_item_size(&c_src, data_items);
                let tgt_size = find_data_item_storage_size(c_target, data_items);
                let copy_size = src_size.min(tgt_size);
                let src_ptr = c_ptr_expr(&c_src, data_items);
                let tgt_ptr = c_ptr_expr(c_target, data_items);
                out.push_str(&format!(
                    "{pad}memcpy({tgt_ptr}, {src_ptr}, {copy_size});\n"
                ));
            }
        } else if let HirExpr::Subscript { variable, .. } = from {
            // Subscripted source to group target: check type and use memcpy
            let c_src = emit_expr(from);
            let src_item = find_data_item(variable.as_str(), data_items);
            let is_src_alpha =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_src_group =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            if is_src_alpha || is_src_group {
                let src_size = find_data_item_size(&sanitize_name(variable), data_items);
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
                let tgt_size = find_data_item_size(c_target, data_items);
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!(
                    "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                     {pad}{{ int64_t _v = {e}; memcpy({tgt_ptr}, &_v, \
                     sizeof(_v) < {tgt_size} ? sizeof(_v) : {tgt_size}); }}\n"
                ));
            }
        } else if let HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } = from
        {
            // Reference-modified source to group target: copy substring
            let c_src = sanitize_name(variable);
            let src_ptr = c_ptr_expr(&c_src, data_items);
            let c_start = emit_expr(start);
            let src_full_size = find_data_item_size(&c_src, data_items);
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
                _ => {
                    // Check if source expr refers to an alpha/group field
                    if is_alpha_expr(from, data_items) || is_group_expr(from, data_items) {
                        let e = emit_expr(from);
                        let src_name = expr_var_name(from);
                        let src_size = find_data_item_size(&sanitize_name(src_name), data_items);
                        let tgt_size = find_data_item_storage_size(c_target, data_items);
                        let copy_size = src_size.min(tgt_size);
                        let src_ptr = if is_group_expr(from, data_items) {
                            c_ptr_expr(&e, data_items)
                        } else {
                            e
                        };
                        let tgt_ptr = c_ptr_expr(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                             {pad}memcpy({tgt_ptr}, {src_ptr}, {copy_size});\n"
                        ));
                    } else {
                        let tgt_ptr = c_ptr_expr(c_target, data_items);
                        let tgt_size = find_data_item_storage_size(c_target, data_items);
                        let e = emit_int_compatible_expr(from, data_items);
                        out.push_str(&format!(
                            "{pad}memset({tgt_ptr}, ' ', {tgt_size});\n\
                             {pad}{{ int64_t _v = {e}; memcpy({tgt_ptr}, &_v, \
                             sizeof(_v) < {tgt_size} ? sizeof(_v) : {tgt_size}); }}\n"
                        ));
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
    let src_var_name = expr_var_name(from);
    let src_type = if !src_var_name.is_empty() {
        find_data_item(src_var_name, data_items).map(|i| &i.data_type)
    } else {
        None
    };
    let is_source_index =
        !src_var_name.is_empty() && src_type.is_none() && is_index_name(src_var_name, data_items);
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
    let is_source_alpha_var = matches!(src_type, Some(HirType::Alphanumeric { .. }));
    let is_source_group_var = matches!(src_type, Some(HirType::Group { .. }));
    let is_source_national_var = matches!(src_type, Some(HirType::National { .. }));

    // National source -> alphanumeric target: use DISPLAY-OF conversion
    if is_target_alpha && is_source_national_var {
        if let HirExpr::Variable(name) = from {
            let c_src = sanitize_name(name);
            let src_size = match find_data_item(name.as_str(), data_items).map(|i| &i.data_type) {
                Some(HirType::National { size }) => *size,
                _ => 1,
            };
            let tgt_size = find_data_item_size(c_target, data_items);
            out.push_str(&format!(
                "{pad}cobol_func_display_of(\
                 (const uint16_t*){c_src}, {src_size}, \
                 (uint8_t*){c_target}, {tgt_size});\n"
            ));
            if !is_group_member_field(c_target) {
                out.push_str(&format!("{pad}{c_target}[{tgt_size}] = '\\0';\n"));
            }
        }
        return;
    }

    match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            if is_target_alpha {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_string((const uint8_t*)\"{escaped}\", {src_len}, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if is_target_group {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_string((const uint8_t*)\"{escaped}\", {src_len}, (uint8_t*)&{c_target}, {tgt_size});\n"
                ));
            } else {
                // Numeric target: parse string as number
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let numval_expr =
                    format!("cobol_func_numval((const uint8_t*)\"{escaped}\", {src_len})");
                emit_store_int(out, c_target, &numval_expr, data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            if is_target_alpha {
                // Numeric literal → alphanumeric: right-justify with leading spaces
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_numeric_to_display({n}, 0, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else {
                emit_store_int(out, c_target, &n.to_string(), data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
                if is_group_member_field(c_target) {
                    out.push_str(&format!("{pad}memset({c_target}, '0', {tgt_size});\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}memset({c_target}, '0', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                    ));
                }
            } else {
                emit_store_int(out, c_target, "0", data_items, pad);
            }
        }
        HirExpr::Literal(HirLiteral::Space) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
                if is_group_member_field(c_target) {
                    out.push_str(&format!("{pad}memset({c_target}, ' ', {tgt_size});\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}memset({c_target}, ' ', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                    ));
                }
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
            out.push_str(&format!(
                "{pad}memset({c_target}, '\"', sizeof({c_target}) - 1);\n"
            ));
            out.push_str(&format!(
                "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Null) => {
            emit_store_int(out, c_target, "0", data_items, pad);
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
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
                            let tgt_size = find_data_item_size(c_target, data_items);
                            let (c_src, src_size_str) = emit_string_func_arg(arg);
                            out.push_str(&format!(
                                "{pad}{{ uint8_t _fbuf[{src_size_str}]; memcpy(_fbuf, (const uint8_t*){c_src}, {src_size_str}); {func}(_fbuf, {src_size_str}); cobol_move_string(_fbuf, {src_size_str}, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        }
                        return;
                    }
                    "CURRENT-DATE" => {
                        let tgt_size = find_data_item_size(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _cdbuf[21]; cobol_func_current_date(_cdbuf, 21); cobol_move_string(_cdbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "WHEN-COMPILED" => {
                        let tgt_size = find_data_item_size(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _wcbuf[21]; cobol_func_when_compiled(_wcbuf, 21); cobol_move_string(_wcbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "CHAR" => {
                        if let Some(arg) = args.first() {
                            let c_arg = emit_expr_as_numeric(arg);
                            let tgt_size = find_data_item_size(c_target, data_items);
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
                            let tgt_size = find_data_item_size(c_target, data_items);
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
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    let src_type = find_data_item(name.as_str(), data_items).map(|i| &i.data_type);
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
            } else if is_target_alpha && is_source_numeric_var {
                // Numeric variable → alphanumeric: use cobol_move_numeric_to_display
                let e = emit_int_compatible_expr(from, data_items);
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_numeric_to_display({e}, 0, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if !is_target_alpha && is_source_alpha_var {
                // Alphanumeric variable/subscript → numeric: use cobol_func_numval
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    let numval_expr =
                        format!("cobol_func_numval((const uint8_t*){c_src}, {src_size})");
                    emit_store_int(out, c_target, &numval_expr, data_items, pad);
                } else if let HirExpr::Subscript { variable, .. } = from {
                    let e = emit_expr(from);
                    let src_size = find_data_item_size(&sanitize_name(variable), data_items);
                    let numval_expr = format!("cobol_func_numval((const uint8_t*){e}, {src_size})");
                    emit_store_int(out, c_target, &numval_expr, data_items, pad);
                } else {
                    let e = emit_expr(from);
                    emit_store_int(out, c_target, &e, data_items, pad);
                }
            } else if is_target_alpha && is_source_group_var {
                // Group variable → alphanumeric: copy bytes with & prefix (group is a C union)
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*)&{c_src}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                }
            } else if is_target_alpha {
                // Alphanumeric → alphanumeric: use cobol_move_string
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){c_src}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if let HirExpr::ReferenceModification {
                    variable,
                    start,
                    length,
                } = from
                {
                    let c_src = sanitize_name(variable);
                    let c_start = emit_expr(start);
                    let src_full_size = find_data_item_size(&c_src, data_items);
                    let c_len = if let Some(len) = length {
                        emit_expr(len)
                    } else {
                        format!("({src_full_size} - ({c_start} - 1))")
                    };
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){c_src} + ({c_start} - 1), {c_len}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if is_source_alpha_var || is_source_group_var {
                    // Subscripted or other alphanumeric/group source
                    let e = emit_expr(from);
                    let src_size = find_data_item_size(&sanitize_name(src_var_name), data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    // When source is a subscript expression the result of
                    // emit_expr is an element value (e.g. char), not a
                    // pointer.  We need to take its address with '&'.
                    let addr_prefix = if matches!(from, HirExpr::Subscript { .. }) {
                        "&"
                    } else {
                        ""
                    };
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){addr_prefix}{e}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else {
                    // Fallback for alpha target with unrecognized source:
                    // use cobol_move_numeric_to_display to safely convert
                    let e = emit_int_compatible_expr(from, data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_numeric_to_display({e}, 0, \
                         (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                }
            } else if is_source_group_var {
                // Group variable → numeric target: treat group as alphanumeric bytes
                // and convert via cobol_func_numval (group is a C union).
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    if is_target_decimal {
                        // Target is CobolDecimal
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int(\
                             cobol_func_numval((const uint8_t*)&{c_src}, {src_size}), \
                             0, &{c_target});\n"
                        ));
                    } else {
                        let numval_expr =
                            format!("cobol_func_numval((const uint8_t*)&{c_src}, {src_size})");
                        emit_store_int(out, c_target, &numval_expr, data_items, pad);
                    }
                }
            } else if is_source_decimal_var {
                // CobolDecimal variable → integer target: use cobol_decimal_to_int64
                let e = emit_expr(from);
                let dec_expr = format!("cobol_decimal_to_int64(&{e})");
                emit_store_int(out, c_target, &dec_expr, data_items, pad);
            } else {
                // Use emit_int_compatible_expr to handle compound expressions
                // that may contain CobolDecimal sub-expressions.
                let e = emit_int_compatible_expr(from, data_items);
                emit_store_int(out, c_target, &e, data_items, pad);
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
    variable: &smol_str::SmolStr,
    start: &HirExpr,
    length: &Option<HirExpr>,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let c_var = sanitize_name(variable);
    let c_start = emit_expr(start);
    let var_size = find_data_item_size(&c_var, data_items);
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
            let c_src = sanitize_name(src_name);
            let src_size = find_data_item_size(&c_src, data_items);
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
            let c_src_var = sanitize_name(src_var);
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

pub(crate) fn emit_perform(
    out: &mut String,
    kind: &HirPerformKind,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
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
            out.push_str(&format!(
                "{pad}for (int64_t _cobol_i = 0; _cobol_i < ({c_count}); _cobol_i++) {{\n"
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
            from,
            by,
            until,
            after_clauses,
            body,
        } => {
            let c_var_target = varying_target_c_expr(var, until);
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
                // Initialize outer VARYING variable
                emit_store_int(out, &c_var_target, &c_from, data_items, &pad);
                let loop_keyword = match test {
                    HirPerformTest::Before => format!("while (!({cond}))"),
                    HirPerformTest::After => "for (;;)".to_string(),
                };
                out.push_str(&format!("{pad}{loop_keyword} {{\n"));
                let after_indent = indent + 1;
                let after_pad = "    ".repeat(after_indent);
                for ac in after_clauses {
                    let ac_var = varying_target_c_expr(&ac.var, &ac.until);
                    let ac_from = emit_int_compatible_expr(&ac.from, data_items);
                    emit_store_int(out, &ac_var, &ac_from, data_items, &after_pad);
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
                for ac in after_clauses.iter().rev() {
                    current_indent -= 1;
                    let ac_var = varying_target_c_expr(&ac.var, &ac.until);
                    let ac_by = emit_int_compatible_expr(&ac.by, data_items);
                    let ac_cond = emit_condition(&ac.until, data_items);
                    let lpad = "    ".repeat(current_indent + 1);
                    if matches!(test, HirPerformTest::After) {
                        out.push_str(&format!("{lpad}if ({ac_cond}) break;\n"));
                    }
                    emit_store_int_op(out, &ac_var, "+", &ac_by, data_items, &lpad);
                    let lpad_close = "    ".repeat(current_indent);
                    out.push_str(&format!("{lpad_close}}}\n"));
                }
                if matches!(test, HirPerformTest::After) {
                    out.push_str(&format!("{after_pad}if ({cond}) break;\n"));
                }
                emit_store_int_op(out, &c_var_target, "+", &c_by, data_items, &after_pad);
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirPerformKind::ProcedureName { name, through } => {
            let c_name = sanitize_name(name);
            let in_body = with_active_context(|ctx| ctx.in_body_context());
            let has_labels = with_active_context(|ctx| ctx.has_labels());
            let need_body_dispatch = in_body && has_labels;
            if let Some(thru) = through {
                // PERFORM name THRU through: call all paragraphs from name to through
                let c_thru = sanitize_name(thru);
                out.push_str(&format!("{pad}/* PERFORM {c_name} THRU {c_thru} */\n"));
                let start_idx = paragraphs
                    .iter()
                    .position(|p| sanitize_name(&p.name) == c_name);
                let end_idx = paragraphs
                    .iter()
                    .position(|p| sanitize_name(&p.name) == c_thru);
                if let (Some(si), Some(ei)) = (start_idx, end_idx) {
                    let (lo, hi) = if si <= ei { (si, ei) } else { (ei, si) };
                    let thru_paras: Vec<_> = paragraphs[lo..=hi]
                        .iter()
                        .map(|p| sanitize_name(&p.name))
                        .collect();

                    if has_labels && thru_paras.len() > 1 {
                        // Generate unique label suffix for this PERFORM THRU
                        let pt_id = with_active_context(|ctx| ctx.next_perform_thru_id());
                        let suffix = format!("pt{pt_id}");
                        // Collect label IDs for paragraphs in the THRU range
                        let thru_ids: Vec<(String, usize)> = with_active_context(|ctx| {
                            thru_paras
                                .iter()
                                .filter_map(|pn| ctx.label_id(pn).map(|id| (pn.clone(), id)))
                                .collect()
                        });

                        // Emit each paragraph call with goto dispatch
                        for (idx, pn) in thru_paras.iter().enumerate() {
                            out.push_str(&format!("_pt_{suffix}_{pn}:\n"));
                            out.push_str(&format!("{pad}para_{pn}();\n"));
                            if idx < thru_paras.len() - 1 {
                                // After each call (except last), check _goto_target
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                ));
                            } else {
                                // After last call, check for out-of-range goto
                                if has_labels {
                                    out.push_str(&format!(
                                        "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                    ));
                                }
                            }
                        }
                        out.push_str(&format!("{pad}goto _pt_end_{suffix};\n"));

                        // Dispatch table for this PERFORM THRU
                        out.push_str(&format!("_pt_disp_{suffix}:\n"));
                        out.push_str(&format!("{pad}{{ int _t = _goto_target;\n"));
                        for (pn, id) in &thru_ids {
                            out.push_str(&format!(
                                "{pad}  if (_t == {id}) {{ _goto_target = 0; goto _pt_{suffix}_{pn}; }}\n"
                            ));
                        }
                        // Not in range: propagate
                        if need_body_dispatch {
                            out.push_str(&format!("{pad}  goto _goto_dispatch;\n"));
                        } else {
                            out.push_str(&format!("{pad}  return;\n"));
                        }
                        out.push_str(&format!("{pad}}}\n"));
                        out.push_str(&format!("_pt_end_{suffix}:;\n"));
                    } else {
                        for pn in &thru_paras {
                            out.push_str(&format!("{pad}para_{pn}();\n"));
                            if need_body_dispatch {
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _goto_dispatch;\n"
                                ));
                            }
                        }
                    }
                } else {
                    // Fallback: just call the named paragraph
                    out.push_str(&format!("{pad}para_{c_name}();\n"));
                    if need_body_dispatch {
                        out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                    }
                }
            } else {
                out.push_str(&format!("{pad}para_{c_name}();\n"));
                if need_body_dispatch {
                    out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                } else if has_labels {
                    // In paragraph function: propagate _goto_target via return
                    out.push_str(&format!("{pad}if (_goto_target) return;\n"));
                }
            }
        }
    }
}

fn varying_target_c_expr(var: &str, until: &HirCondition) -> String {
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
        HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            if is_decimal_expr(expr, data_items) {
                let c_expr = emit_expr(expr);
                let scale = decimal_item_scale(expr_var_name(expr), data_items)?;
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
    let target_scale = match decimal_item_scale(expr_var_name(target), data_items) {
        Some(scale) => scale,
        None => return false,
    };
    let scaled_operand = match decimal_expr_as_scaled_int64(operand, target_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    out.push_str(&format!("{pad}{c_target}.value += ({scaled_operand});\n"));
    true
}

fn emit_fast_decimal_multiply_giving(
    out: &mut String,
    c_target: &str,
    target: &HirExpr,
    operand: &HirExpr,
    by_operand: Option<&HirExpr>,
    data_items: &[HirDataItem],
    pad: &str,
) -> bool {
    let target_scale = match decimal_item_scale(expr_var_name(target), data_items) {
        Some(scale) => scale,
        None => return false,
    };
    let by_operand = match by_operand {
        Some(expr) => expr,
        None => return false,
    };
    let left = match decimal_expr_as_scaled_int64(operand, target_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    let right = match decimal_expr_as_scaled_int64(by_operand, target_scale, data_items) {
        Some(expr) => expr,
        None => return false,
    };
    let divisor = pow10_i64_literal(target_scale);
    out.push_str(&format!(
        "{pad}{c_target}.value = (((int64_t)({left})) * ((int64_t)({right}))) / {divisor};\n"
    ));
    true
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
        HirExpr::Subscript { variable, .. } if variable == var => Some(expr),
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
        HirExpr::Variable(name) | HirExpr::Subscript { variable: name, .. } => {
            let c_name = match expr {
                HirExpr::Variable(_) => sanitize_name(name),
                _ => emit_expr(expr),
            };
            let base_name = sanitize_name(name);
            let leaf_name = extract_leaf_member(&c_name);
            let is_dec = ctx.is_decimal_name(&base_name) || ctx.is_decimal_name(leaf_name);
            let is_grp = ctx.is_group_name(&base_name) || ctx.is_group_name(leaf_name);
            if is_dec {
                format!("cobol_decimal_to_int64(&{c_name})")
            } else if is_grp {
                // Group variables are C unions; cast to 0 in numeric context
                // (groups used in arithmetic are unusual; default to 0).
                "((int64_t)0)".to_string()
            } else {
                let disp_size = grp_display_size(&c_name, &[])
                    .or_else(|| with_active_context(|ctx| ctx.display_numeric_size(&base_name)));
                if let Some(size) = disp_size {
                    let c_name_ptr = display_numeric_const_ptr(&c_name);
                    format!("cobol_display_to_int64({c_name_ptr}, {size})")
                } else {
                    c_name
                }
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
        HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            let c_name = super::emit_expr_with_ctx(expr, ctx);
            let var_name = expr_var_name(expr);
            let base_name = sanitize_name(var_name);
            let leaf_name = extract_leaf_member(&c_name);
            let is_dec = (!base_name.is_empty() && ctx.is_decimal_name(&base_name))
                || ctx.is_decimal_name(leaf_name);
            if is_dec {
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
                    format!("(double)cobol_display_to_int64({c_name_ptr}, {size})")
                } else if !base_name.is_empty()
                    && (c_name.contains('[') || c_name.contains(".members._m_"))
                    && ctx
                        .data_item_size(&base_name)
                        .or_else(|| ctx.data_item_size(leaf_name))
                        .is_some()
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
        HirExpr::Literal(HirLiteral::Decimal(d)) => d.to_string(),
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
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let ptr = c_ptr_expr(&c_name, data_items);
            let len = find_data_item_size(&c_name, data_items).to_string();
            (format!("(const uint8_t*)(const void*){ptr}"), len)
        }
        HirExpr::Subscript { .. } => {
            let c_name = emit_expr(expr);
            let ptr = format!("(const uint8_t*)(const void*){c_name}");
            let len = find_data_item_size(expr_var_name(expr), data_items).to_string();
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
            "{pad}cobol_move_string((const uint8_t*)((({rc_expr}) == 0) ? \"00\" : (({rc_expr}) == 10 ? \"10\" : (({rc_expr}) == 20 ? \"20\" : (({rc_expr}) == 21 ? \"21\" : (({rc_expr}) == 30 ? \"30\" : (({rc_expr}) == 40 ? \"40\" : (({rc_expr}) == 60 ? \"60\" : \"99\"))))))), 2, (uint8_t*){}, {});\n",
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
        emit_store_int(
            out,
            &text_length,
            &format!("(int64_t){text_len_expr}"),
            data_items,
            pad,
        );
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
            "{pad}if (({rc_expr}) != 20) cobol_move_string((const uint8_t*)((({rc_expr}) != 0 && ({rc_expr}) != 10) ? \"1\" : \"0\"), 1, (uint8_t*){}, {});\n",
            c_ptr_expr(&error_key, data_items),
            find_data_item_size(&error_key, data_items)
        ));
    }
    if let Some(symbolic_source) = binding.symbolic_source {
        out.push_str(&format!(
            "{pad}memset({}, ' ', {});\n",
            c_ptr_expr(&symbolic_source, data_items),
            find_data_item_size(&symbolic_source, data_items)
        ));
    }
    if let Some(destination_count) = binding.destination_count {
        emit_store_int(out, &destination_count, "0", data_items, pad);
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
            find_data_item_size(name, data_items).to_string(),
        )
    })
    .unwrap_or_else(null_comm_arg)
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
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            (c_name.clone(), format!("sizeof({c_name})"))
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
