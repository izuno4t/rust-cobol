use super::*;

pub(crate) fn emit_data_items(
    out: &mut String,
    items: &[HirDataItem],
    fd_aliases: &HashSet<String>,
) {
    if items.is_empty() {
        return;
    }
    // Collect names that are members of groups (emitted inside struct).
    // These should be skipped when they appear as top-level items to avoid
    // redefinition conflicts with the #define macros.
    let group_member_names = collect_group_member_names(items);
    // Collect member names that appear in multiple groups — these should
    // NOT get unqualified #define macros to avoid redefinition warnings.
    let duplicate_member_names = collect_duplicate_member_names(items, &group_member_names);

    let mut emitted_typedefs = HashSet::new();
    out.push_str("/* Data items */\n");
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue; // Already emitted as part of a group struct
        }
        if fd_aliases.contains(&c_name) {
            continue; // FD record alias — will be #defined to the primary record
        }
        emit_single_data_item(out, item, &duplicate_member_names, &mut emitted_typedefs);
    }
    out.push('\n');
}

/// Collect sanitized names of all items that are members of a group.
pub(crate) fn collect_group_member_names(items: &[HirDataItem]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for item in items {
        if let HirType::Group { members, .. } = &item.data_type {
            collect_member_names_recursive(members, &mut names);
        }
    }
    names
}

pub(crate) fn collect_member_names_recursive(members: &[HirDataItem], names: &mut BTreeSet<String>) {
    for member in members {
        names.insert(sanitize_name(&member.name));
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_member_names_recursive(sub_members, names);
        }
    }
}

/// Collect member names that appear in more than one top-level group.
/// These names should only get qualified #define macros, not unqualified ones.
/// Sub-groups that are members of other groups are excluded to avoid false duplicates.
pub(crate) fn collect_duplicate_member_names(
    items: &[HirDataItem],
    group_member_names: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for item in items {
        let c_name = sanitize_name(&item.name);
        // Skip sub-groups that are members of other groups
        if group_member_names.contains(&c_name) {
            continue;
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let mut group_names = BTreeSet::new();
            collect_member_names_recursive(members, &mut group_names);
            for name in &group_names {
                if !seen.insert(name.clone()) {
                    duplicates.insert(name.clone());
                }
            }
        }
    }
    duplicates
}

pub(crate) fn emit_single_data_item(
    out: &mut String,
    item: &HirDataItem,
    duplicate_member_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
) {
    let c_name = sanitize_name(&item.name);

    // RENAMES (level 66): emit a #define alias, no variable declaration
    if let Some((ref from, ref _thru)) = item.renames {
        let c_from = sanitize_name(from);
        out.push_str(&format!(
            "#define {c_name} {c_from} /* RENAMES {c_from} */\n"
        ));
        return;
    }

    // REDEFINES: overlay on another item's memory via #define with cast
    if let Some(ref redef_name) = item.redefines {
        let c_redef = sanitize_name(redef_name);
        let c_type = c_type_for_hir_type(&item.data_type);
        match &item.data_type {
            HirType::Alphanumeric { .. } | HirType::National { .. } => {
                // Array types: cast to pointer (acts as array base for memset/strncpy)
                out.push_str(&format!(
                    "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                ));
            }
            HirType::Group { members, .. } => {
                // Group REDEFINES: reinterpret as the group's struct type
                emit_group_typedefs(out, &c_name, members, emitted_typedefs);
                let td = group_typedef_name(&c_name, members);
                out.push_str(&format!(
                    "#define {c_name} (*({td}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                ));
                // Emit #define macros for children of this REDEFINES group
                // Note: REDEFINES group is a struct, not a union, so no .members wrapper
                emit_group_macros(
                    out,
                    members,
                    std::slice::from_ref(&c_name),
                    &c_name,
                    duplicate_member_names,
                );
                // Emit nested REDEFINES within this top-level REDEFINES group
                emit_group_redefines(
                    out,
                    members,
                    &c_name,
                    duplicate_member_names,
                    emitted_typedefs,
                );
            }
            _ => {
                if item.occurs.is_some() {
                    // REDEFINES + OCCURS: cast to pointer so it acts as an array base
                    out.push_str(&format!(
                        "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} OCCURS */\n"
                    ));
                } else {
                    // Scalar types: dereference cast for lvalue semantics
                    out.push_str(&format!(
                        "#define {c_name} (*({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                    ));
                }
            }
        }
        return;
    }

    let array_suffix = if let Some(n) = item.occurs {
        format!("[{n}]")
    } else {
        String::new()
    };
    match &item.data_type {
        HirType::Alphanumeric { size } => {
            if item.occurs.is_some() {
                out.push_str(&format!(
                    "static char {c_name}{array_suffix}[{}];\n",
                    size + 1
                ));
            } else {
                out.push_str(&format!("static char {}[{}];\n", c_name, size + 1));
            }
        }
        HirType::National { size } => {
            if item.occurs.is_some() {
                out.push_str(&format!(
                    "static uint16_t {c_name}{array_suffix}[{size}];\n"
                ));
            } else {
                out.push_str(&format!("static uint16_t {c_name}[{size}];\n"));
            }
        }
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("static CobolDecimal {c_name}{array_suffix};\n"));
        }
        HirType::Numeric { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Group { members, .. } => {
            // Emit group as union of struct + byte array for group-level operations
            emit_group_typedefs(out, &c_name, members, emitted_typedefs);
            let td = group_typedef_name(&c_name, members);
            out.push_str("static union {\n");
            out.push_str(&format!("    {td} members;\n"));
            out.push_str(&format!("    uint8_t _bytes[sizeof({td})];\n"));
            out.push_str(&format!("}} {c_name};\n"));
            // Generate macros for group members.
            // Qualified: #define GROUP__FIELD_A GROUP.members._m_FIELD_A (always unique)
            // Unqualified: #define FIELD_A ... (only if name is unique across groups)
            emit_group_macros(
                out,
                members,
                std::slice::from_ref(&c_name),
                &format!("{c_name}.members"),
                duplicate_member_names,
            );
            // Emit REDEFINES members as separate static pointers
            emit_group_redefines(
                out,
                members,
                &format!("{c_name}.members"),
                duplicate_member_names,
                emitted_typedefs,
            );
            out.push('\n');
        }
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("static CobolDecimal {c_name}{array_suffix};\n"));
        }
        HirType::Comp3 { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Binary { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Index => {
            out.push_str(&format!("static int64_t {c_name};\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("static void* {c_name};\n"));
        }
        HirType::Boolean => {
            out.push_str(&format!("static int8_t {c_name}{array_suffix};\n"));
        }
        HirType::FloatShort => {
            out.push_str(&format!("static float {c_name}{array_suffix};\n"));
        }
        HirType::FloatLong => {
            out.push_str(&format!("static double {c_name}{array_suffix};\n"));
        }
        HirType::FloatExtended => {
            out.push_str(&format!("static long double {c_name}{array_suffix};\n"));
        }
    }
}

/// Emit struct typedef(s) for a group and its nested groups (bottom-up).
pub(crate) fn emit_group_typedefs(
    out: &mut String,
    c_name: &str,
    members: &[HirDataItem],
    emitted_typedefs: &mut HashSet<String>,
) {
    // First, recurse into nested groups
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            let member_c_name = sanitize_name(&member.name);
            emit_group_typedefs(out, &member_c_name, sub_members, emitted_typedefs);
        }
    }
    // Skip if this exact typedef (name + member layout) has already been emitted
    let typedef_name = group_typedef_name(c_name, members);
    if !emitted_typedefs.insert(typedef_name.clone()) {
        return;
    }
    // Emit this level's struct typedef
    out.push_str("typedef struct {\n");
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if member.redefines.is_some() {
            continue; // REDEFINES handled separately
        }
        emit_group_struct_member(out, member, &mut member_name_counts);
    }
    out.push_str(&format!("}} {typedef_name};\n"));
}

/// Emit a single member within a group struct typedef.
pub(crate) fn emit_group_struct_member(
    out: &mut String,
    member: &HirDataItem,
    member_name_counts: &mut HashMap<String, u32>,
) {
    let base_c_name = sanitize_name(&member.name);
    // Track member names to avoid duplicates (common with FILLER and implicit FILLER items)
    let count = member_name_counts.entry(base_c_name.clone()).or_insert(0);
    *count += 1;
    let c_name = if *count > 1 {
        format!("{}_{}", base_c_name, count)
    } else {
        base_c_name
    };
    let array_suffix = member.occurs.map_or(String::new(), |n| format!("[{n}]"));
    match &member.data_type {
        HirType::Alphanumeric { size } => {
            // In group structs, do NOT add +1 for null terminator.
            // Group members must match COBOL byte layout exactly so that
            // group-level MOVE/COMPARE operations work correctly.
            if member.occurs.is_some() {
                out.push_str(&format!("    char _m_{c_name}{array_suffix}[{size}];\n"));
            } else {
                out.push_str(&format!("    char _m_{c_name}[{size}];\n"));
            }
        }
        HirType::National { size } => {
            if member.occurs.is_some() {
                out.push_str(&format!(
                    "    uint16_t _m_{c_name}{array_suffix}[{size}];\n"
                ));
            } else {
                out.push_str(&format!("    uint16_t _m_{c_name}[{size}];\n"));
            }
        }
        HirType::Numeric {
            size,
            decimal_places,
            ..
        } if *decimal_places > 0 => {
            out.push_str(&format!("    CobolDecimal _m_{c_name}{array_suffix};\n"));
        }
        HirType::Numeric { size, .. } => {
            // USAGE DISPLAY numeric in group: store as zoned decimal (char[])
            // so that group-level byte operations (MOVE, COMPARE) work correctly.
            let disp_size = *size as usize;
            out.push_str(&format!(
                "    char _m_{c_name}{array_suffix}[{disp_size}];\n"
            ));
        }
        HirType::Group {
            members: ref sub_members,
            ..
        } => {
            let td = group_typedef_name(&c_name, sub_members);
            out.push_str(&format!(
                "    union {{ {td} members; uint8_t _bytes[sizeof({td})]; }} _m_{c_name}{array_suffix};\n"
            ));
        }
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("    CobolDecimal _m_{c_name}{array_suffix};\n"));
        }
        HirType::Comp3 { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Binary { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Index => {
            out.push_str(&format!("    int64_t _m_{c_name};\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("    void* _m_{c_name};\n"));
        }
        HirType::Boolean => {
            out.push_str(&format!("    int8_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatShort => {
            out.push_str(&format!("    float _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatLong => {
            out.push_str(&format!("    double _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatExtended => {
            out.push_str(&format!("    long double _m_{c_name}{array_suffix};\n"));
        }
    }
}

/// Emit #define macros for all elementary members in a group.
pub(crate) fn emit_group_macros(
    out: &mut String,
    members: &[HirDataItem],
    qualifier_names: &[String],
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
) {
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        // FILLER items (and items misnamed "PIC" from implicit FILLER) are unnamed
        // padding; skip macro generation to avoid duplicate #define errors
        if member.name == "FILLER" || member.name == "PIC" {
            continue;
        }
        let c_name = sanitize_name(&member.name);
        let access_path = format!("{path_prefix}._m_{c_name}");
        // Unqualified macro: only if name is unique across all groups
        if !duplicate_names.contains(&c_name) {
            out.push_str(&format!("#define {c_name} {access_path}\n"));
        }
        // Qualified macros: QUALIFIER__FIELD for each ancestor group name.
        // This supports COBOL qualified references like FIELD OF GROUP-A,
        // FIELD OF GROUP-B, etc.
        for qualifier in qualifier_names {
            out.push_str(&format!("#define {qualifier}__{c_name} {access_path}\n"));
        }
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            // For OCCURS items, child macros access element [0] (first
            // element) as a safe default.  Subscripted access is handled
            // by emit_subscript_access which generates proper indexed
            // paths at each OCCURS level.
            let sub_prefix = if member.occurs.is_some() {
                format!("{access_path}[0].members")
            } else {
                format!("{access_path}.members")
            };
            // Add current member name as qualifier for children, then
            // recurse once (avoids exponential blowup from double recursion).
            let mut child_qualifiers = qualifier_names.to_vec();
            if !child_qualifiers.contains(&c_name) {
                child_qualifiers.push(c_name);
            }
            emit_group_macros(
                out,
                sub_members,
                &child_qualifiers,
                &sub_prefix,
                duplicate_names,
            );
        }
    }
}

/// Emit REDEFINES members within a group as #define macros with qualified paths.
pub(crate) fn emit_group_redefines(
    out: &mut String,
    members: &[HirDataItem],
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
) {
    for member in members {
        if let Some(ref redef_name) = member.redefines {
            let c_name = sanitize_name(&member.name);
            let c_redef = sanitize_name(redef_name);
            let c_type = c_type_for_hir_type(&member.data_type);
            let qualified_target = format!("{path_prefix}._m_{c_redef}");
            match &member.data_type {
                HirType::Alphanumeric { .. } | HirType::National { .. } => {
                    out.push_str(&format!(
                        "#define {c_name} (({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */\n"
                    ));
                }
                HirType::Group {
                    members: grp_members,
                    ..
                } => {
                    emit_group_typedefs(out, &c_name, grp_members, emitted_typedefs);
                    let td = group_typedef_name(&c_name, grp_members);
                    out.push_str(&format!(
                        "#define {c_name} (*({td}*)&{qualified_target}) /* REDEFINES {c_redef} */\n"
                    ));
                    emit_group_macros(
                        out,
                        grp_members,
                        std::slice::from_ref(&c_name),
                        &c_name,
                        duplicate_names,
                    );
                    // Recurse into REDEFINES group children to emit nested
                    // REDEFINES macros (e.g. RDF3-5-1 REDEFINES RDF3-5).
                    let sub_prefix = c_name.clone();
                    emit_group_redefines(
                        out,
                        grp_members,
                        &sub_prefix,
                        duplicate_names,
                        emitted_typedefs,
                    );
                }
                _ => {
                    if member.occurs.is_some() {
                        // REDEFINES + OCCURS: pointer cast (acts as array base)
                        out.push_str(&format!(
                            "#define {c_name} (({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} OCCURS */\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "#define {c_name} (*({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */\n"
                        ));
                    }
                }
            }
        }
        if member.redefines.is_none() {
            if let HirType::Group {
                members: sub_members,
                ..
            } = &member.data_type
            {
                let c_name = sanitize_name(&member.name);
                let sub_prefix = if member.occurs.is_some() {
                    format!("{path_prefix}._m_{c_name}[0].members")
                } else {
                    format!("{path_prefix}._m_{c_name}.members")
                };
                emit_group_redefines(
                    out,
                    sub_members,
                    &sub_prefix,
                    duplicate_names,
                    emitted_typedefs,
                );
            }
        }
    }
}

/// Return the C type string for a given HIR type.
pub(crate) fn c_type_for_hir_type(ty: &HirType) -> &'static str {
    match ty {
        HirType::Alphanumeric { .. } => "char",
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => "CobolDecimal",
        HirType::Numeric { .. } => "int64_t",
        HirType::Group { .. } => "char",
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => "CobolDecimal",
        HirType::Comp3 { .. } => "int64_t",
        HirType::Binary { .. } => "int64_t",
        HirType::Index => "int64_t",
        HirType::Pointer => "void",
        HirType::Boolean => "int8_t",
        HirType::FloatShort => "float",
        HirType::FloatLong => "double",
        HirType::FloatExtended => "long double",
        HirType::National { .. } => "uint16_t",
    }
}

pub(crate) fn emit_data_init(out: &mut String, items: &[HirDataItem]) {
    // Skip top-level items that are already members of a group
    // (they are initialized through the group's recursive init)
    let group_member_names = collect_group_member_names(items);
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue;
        }
        // Skip REDEFINES items — they share memory with the redefined item
        if item.redefines.is_some() {
            continue;
        }
        emit_single_data_init(out, item);
    }
}

pub(crate) fn emit_single_data_init(out: &mut String, item: &HirDataItem) {
    emit_single_data_init_with_prefix(out, item, None, None);
}

pub(crate) fn emit_single_data_init_with_prefix(
    out: &mut String,
    item: &HirDataItem,
    group_prefix: Option<&str>,
    disambiguated_name: Option<&str>,
) {
    let base_c_name =
        disambiguated_name.map_or_else(|| sanitize_name(&item.name), |s| s.to_string());
    // Use C struct access path when inside a group, not macro names
    let c_name = if let Some(prefix) = group_prefix {
        format!("{prefix}._m_{base_c_name}")
    } else {
        base_c_name.clone()
    };
    if let HirType::Group { members, .. } = &item.data_type {
        // If this group itself has OCCURS, zero-init the entire array of structs
        // rather than recursing into members (which would fail because we can't
        // access .members on an array element without a subscript).
        if item.occurs.is_some() {
            out.push_str(&format!("    memset(&{c_name}, 0, sizeof({c_name}));\n"));
            return;
        }
        // Initialize group members recursively with C struct access path
        let my_prefix = if let Some(prefix) = group_prefix {
            format!("{prefix}._m_{base_c_name}.members")
        } else {
            format!("{base_c_name}.members")
        };
        // Track member name counts to match the struct member naming
        // (e.g., FILLER -> _m_FILLER, _m_FILLER_2, _m_FILLER_3)
        let mut member_name_counts: HashMap<String, u32> = HashMap::new();
        for member in members {
            // Skip REDEFINES members — they share memory with the redefined item
            if member.redefines.is_some() {
                continue;
            }
            let member_base = sanitize_name(&member.name);
            let count = member_name_counts.entry(member_base.clone()).or_insert(0);
            *count += 1;
            let deduped = if *count > 1 {
                format!("{}_{}", member_base, count)
            } else {
                member_base
            };
            emit_single_data_init_with_prefix(out, member, Some(&my_prefix), Some(&deduped));
        }
        return;
    }
    // OCCURS items: zero-initialize the entire array
    if let Some(n) = item.occurs {
        match &item.data_type {
            HirType::Numeric { .. }
            | HirType::Comp3 { .. }
            | HirType::Binary { .. }
            | HirType::Boolean => {
                out.push_str(&format!("    memset({c_name}, 0, sizeof({c_name}));\n"));
            }
            HirType::Alphanumeric { size } => {
                if group_prefix.is_some() {
                    out.push_str(&format!(
                        "    for (int _i = 0; _i < {n}; _i++) {{ memset({c_name}[_i], ' ', {size}); }}\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    for (int _i = 0; _i < {n}; _i++) {{ memset({c_name}[_i], ' ', {size}); {c_name}[_i][{size}] = '\\0'; }}\n"
                    ));
                }
            }
            HirType::National { size } => {
                out.push_str(&format!(
                    "    for (int _i = 0; _i < {n}; _i++) {{ for (uint32_t _j = 0; _j < {size}; _j++) {{ {c_name}[_i][_j] = 0x0020; }} }}\n"
                ));
            }
            _ => {
                out.push_str(&format!("    memset({c_name}, 0, sizeof({c_name}));\n"));
            }
        }
        return;
    }
    // CobolDecimal initialization
    if needs_decimal(&item.data_type) {
        let (size, decimal_places, is_signed) = match &item.data_type {
            HirType::Numeric {
                size,
                decimal_places,
                is_signed,
            } => (*size, *decimal_places, *is_signed),
            HirType::Comp3 {
                size,
                decimal_places,
            } => (*size, *decimal_places, true),
            _ => unreachable!(),
        };
        if let Some(init) = &item.initial_value {
            match init {
                HirLiteral::Integer(n) => {
                    // Integer VALUE for a decimal field: scale up
                    let scale = decimal_places;
                    let scaled = *n * 10_i64.pow(scale);
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = {scaled}, .scale = {scale}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                HirLiteral::Decimal(d) => {
                    // Parse decimal literal: "123.45" -> value=12345, scale=2
                    let (value, scale) = parse_decimal_literal(d);
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = {value}, .scale = {scale}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                HirLiteral::Zero => {
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
            }
        } else {
            out.push_str(&format!(
                "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                if is_signed { 1 } else { 0 }
            ));
        }
        return;
    }
    let in_group = group_prefix.is_some();
    if let Some(init) = &item.initial_value {
        match (&item.data_type, init) {
            (HirType::Alphanumeric { size }, HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                if in_group {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    strncpy({c_name}, \"{escaped}\", {size});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    strncpy({c_name}, \"{escaped}\", {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Space) => {
                if in_group {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Zero) => {
                if in_group {
                    out.push_str(&format!(
                        "    memset({c_name}, '0', {size});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, '0', {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places: 0,
                    ..
                },
                HirLiteral::Integer(n),
            ) if group_prefix.is_some() => {
                out.push_str(&format!(
                    "    cobol_store_numeric_display({n}, \
                     (uint8_t*){c_name}, {size});\n"
                ));
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places: 0,
                    ..
                },
                HirLiteral::Zero,
            ) if group_prefix.is_some() => {
                out.push_str(&format!(
                    "    cobol_store_numeric_display(0, \
                     (uint8_t*){c_name}, {size});\n"
                ));
            }
            (
                HirType::Numeric { .. }
                | HirType::Index
                | HirType::Comp3 { .. }
                | HirType::Binary { .. }
                | HirType::Boolean
                | HirType::FloatShort
                | HirType::FloatLong
                | HirType::FloatExtended,
                HirLiteral::Integer(n),
            ) => {
                out.push_str(&format!("    {c_name} = {n};\n"));
            }
            (
                HirType::Numeric { .. }
                | HirType::Index
                | HirType::Comp3 { .. }
                | HirType::Binary { .. }
                | HirType::Boolean
                | HirType::FloatShort
                | HirType::FloatLong
                | HirType::FloatExtended,
                HirLiteral::Zero,
            ) => {
                out.push_str(&format!("    {c_name} = 0;\n"));
            }
            (HirType::National { size }, HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                out.push_str(&format!(
                    "    cobol_move_to_national((const uint8_t*)\"{escaped}\", {src_len}, {c_name}, {size});\n"
                ));
            }
            (HirType::National { size }, HirLiteral::Space) => {
                out.push_str(&format!(
                    "    for (uint32_t _i = 0; _i < {size}; _i++) {{ {c_name}[_i] = 0x0020; }}\n"
                ));
            }
            _ => {
                emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
            }
        }
    } else {
        emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
    }
}

pub(crate) fn emit_default_init(out: &mut String, data_type: &HirType, c_name: &str, in_group: bool) {
    match data_type {
        HirType::Alphanumeric { size } => {
            if in_group {
                out.push_str(&format!(
                    "    memset({c_name}, ' ', {size});\n"
                ));
            } else {
                out.push_str(&format!(
                    "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
                ));
            }
        }
        HirType::Numeric {
            size,
            decimal_places: 0,
            ..
        } if in_group => {
            out.push_str(&format!(
                "    cobol_store_numeric_display(0, (uint8_t*){c_name}, {size});\n"
            ));
        }
        HirType::Numeric { .. }
        | HirType::Index
        | HirType::Comp3 { .. }
        | HirType::Binary { .. }
        | HirType::Boolean => {
            out.push_str(&format!("    {c_name} = 0;\n"));
        }
        HirType::FloatShort | HirType::FloatLong | HirType::FloatExtended => {
            out.push_str(&format!("    {c_name} = 0.0;\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("    {c_name} = NULL;\n"));
        }
        HirType::National { size } => {
            // Fill with UTF-16 spaces (0x0020)
            out.push_str(&format!(
                "    for (uint32_t _i = 0; _i < {size}; _i++) {{ {c_name}[_i] = 0x0020; }}\n"
            ));
        }
        HirType::Group { members, .. } => {
            for member in members {
                emit_single_data_init(out, member);
            }
        }
    }
}
