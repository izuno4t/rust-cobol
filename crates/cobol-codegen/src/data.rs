use super::*;
use cobol_hir::HirSignPosition;
use std::cell::RefCell;

thread_local! {
    static GLOBAL_EMITTED_GROUP_TYPEDEFS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static GLOBAL_EMITTED_EXTERNAL_ITEMS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub(crate) fn reset_group_typedef_registry() {
    GLOBAL_EMITTED_GROUP_TYPEDEFS.with(|registry| registry.borrow_mut().clear());
    GLOBAL_EMITTED_EXTERNAL_ITEMS.with(|registry| registry.borrow_mut().clear());
}

pub(crate) fn emit_fd_alias_macros(
    out: &mut String,
    items: &[HirDataItem],
    fd_record_aliases: &std::collections::HashMap<smol_str::SmolStr, smol_str::SmolStr>,
) {
    if fd_record_aliases.is_empty() {
        return;
    }
    let group_member_names = collect_group_member_names(items);
    let duplicate_member_names = collect_duplicate_member_names(items, &group_member_names);
    let mut emitted_typedefs = HashSet::new();
    out.push_str("/* FD record aliases (shared record area) */\n");
    for (alias, primary) in fd_record_aliases {
        let c_alias = sanitize_name(alias);
        let c_primary = sanitize_name(primary);
        let primary_item = items
            .iter()
            .find(|item| item.name.eq_ignore_ascii_case(primary));
        let Some(alias_item) = items
            .iter()
            .find(|item| item.name.eq_ignore_ascii_case(alias))
        else {
            if matches!(
                primary_item.map(|item| &item.data_type),
                Some(HirType::Group { .. })
            ) {
                out.push_str(&format!(
                    "#define {c_alias} ((char*){c_primary}._bytes) /* FD shared record area */\n"
                ));
            } else {
                out.push_str(&format!(
                    "#define {c_alias} {c_primary} /* FD shared record area */\n"
                ));
            }
            continue;
        };
        if let HirType::Group { members, .. } = &alias_item.data_type {
            emit_group_typedefs(out, &c_alias, members, &mut emitted_typedefs, true);
            let td = group_typedef_name_for_layout(&c_alias, members, true);
            let union_td = format!("_fd_alias_{c_alias}_t");
            out.push_str(&format!(
                "typedef union {{ {td} members; uint8_t _bytes[sizeof({td})]; }} {union_td};\n"
            ));
            out.push_str(&format!(
                "#define {c_alias} (*({union_td}*)&{c_primary}) /* FD shared record area */\n"
            ));
            emit_group_macros(
                out,
                members,
                std::slice::from_ref(&c_alias),
                &format!("{c_alias}.members"),
                &duplicate_member_names,
            );
            emit_group_redefines(
                out,
                members,
                std::slice::from_ref(&c_alias),
                &format!("{c_alias}.members"),
                &duplicate_member_names,
                &mut emitted_typedefs,
                true,
            );
        } else if matches!(
            primary_item.map(|item| &item.data_type),
            Some(HirType::Group { .. })
        ) {
            out.push_str(&format!(
                "#define {c_alias} ((char*){c_primary}._bytes) /* FD shared record area */\n"
            ));
        } else {
            out.push_str(&format!(
                "#define {c_alias} {c_primary} /* FD shared record area */\n"
            ));
        }
    }
    out.push('\n');
}

pub(crate) fn emit_data_items(
    out: &mut String,
    items: &[HirDataItem],
    fd_aliases: &HashSet<String>,
    fd_record_aliases: &std::collections::HashMap<smol_str::SmolStr, smol_str::SmolStr>,
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

    // Compute the maximum record size for each primary FD record.
    // When multiple 01-level records share the same FD, the primary record's
    // static union must be large enough to hold any of them.
    let fd_primary_max_sizes = compute_fd_primary_max_sizes(items, fd_record_aliases);
    let redefines_max_sizes = compute_redefines_max_sizes(items);

    let mut emitted_typedefs = HashSet::new();
    out.push_str("/* Data items */\n");
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) && item.redefines.is_none() {
            continue; // Already emitted as part of a group struct
        }
        if fd_aliases.contains(&c_name) {
            continue; // FD record alias — will be #defined to the primary record
        }
        let fd_max = fd_primary_max_sizes.get(&c_name).copied();
        let redef_max = redefines_max_sizes.get(&c_name).copied();
        let storage_max = match (fd_max, redef_max) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        emit_single_data_item(
            out,
            item,
            items,
            &duplicate_member_names,
            &mut emitted_typedefs,
            storage_max,
        );
    }
    out.push('\n');
}

/// Compute max record size for each primary FD record name.
/// Returns a map from sanitized primary record name → max byte size across
/// all 01-level records in the same FD.
fn compute_fd_primary_max_sizes(
    items: &[HirDataItem],
    fd_record_aliases: &std::collections::HashMap<smol_str::SmolStr, smol_str::SmolStr>,
) -> HashMap<String, u32> {
    use super::find_data_item_size;

    if fd_record_aliases.is_empty() {
        return HashMap::new();
    }

    // Build reverse map: primary_name → vec of alias names (unsanitized)
    let mut primary_to_aliases: HashMap<String, Vec<String>> = HashMap::new();
    for (alias, primary) in fd_record_aliases {
        let c_primary = sanitize_name(primary);
        let c_alias = sanitize_name(alias);
        primary_to_aliases
            .entry(c_primary)
            .or_default()
            .push(c_alias);
    }

    let mut result = HashMap::new();
    for (c_primary, aliases) in &primary_to_aliases {
        let primary_size = find_data_item_size(c_primary, items);
        let mut max_size = primary_size;
        for c_alias in aliases {
            let alias_size = find_data_item_size(c_alias, items);
            if alias_size > max_size {
                max_size = alias_size;
            }
        }
        // Only store if max_size exceeds the primary record's own size
        if max_size > primary_size {
            result.insert(c_primary.clone(), max_size);
        }
    }
    result
}

fn compute_redefines_max_sizes(items: &[HirDataItem]) -> HashMap<String, u32> {
    let mut result = HashMap::new();
    for item in items {
        let Some(redef_name) = &item.redefines else {
            continue;
        };
        let c_redef = sanitize_name(redef_name);
        let alias_size = data_item_storage_size(item);
        let primary_size = items
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(redef_name))
            .map(data_item_storage_size)
            .unwrap_or(0);
        let max_size = alias_size.max(primary_size);
        result
            .entry(c_redef)
            .and_modify(|size: &mut u32| *size = (*size).max(max_size))
            .or_insert(max_size);
    }
    result
}

pub(crate) fn group_needs_raw_display_layout(
    item: &HirDataItem,
    all_items: &[HirDataItem],
) -> bool {
    if item.redefines.is_some() {
        return true;
    }
    let c_name = sanitize_name(&item.name);
    all_items
        .iter()
        .any(|candidate| candidate.redefines.as_deref().map(sanitize_name) == Some(c_name.clone()))
        || match &item.data_type {
            HirType::Group { members, .. } => group_members_need_raw_display_layout(members),
            _ => false,
        }
}

pub(crate) fn group_members_need_raw_display_layout(members: &[HirDataItem]) -> bool {
    members.iter().any(|member| {
        member.redefines.is_some()
            || match &member.data_type {
                HirType::Numeric { .. } => true,
                HirType::Group {
                    members: sub_members,
                    ..
                } => group_members_need_raw_display_layout(sub_members),
                _ => false,
            }
    })
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

pub(crate) fn collect_member_names_recursive(
    members: &[HirDataItem],
    names: &mut BTreeSet<String>,
) {
    for member in members {
        names.insert(sanitize_name(&member.name));
        if member.renames.is_some() {
            continue;
        }
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_member_names_recursive(sub_members, names);
        }
    }
}

pub(crate) fn find_group_member_by_sanitized_name<'a>(
    c_name: &str,
    members: &'a [HirDataItem],
) -> Option<&'a HirDataItem> {
    for member in members {
        if sanitize_name(&member.name) == c_name {
            return Some(member);
        }
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            if let Some(found) = find_group_member_by_sanitized_name(c_name, sub_members) {
                return Some(found);
            }
        }
    }
    None
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
        if group_member_names.contains(&c_name) && item.redefines.is_none() {
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

fn top_level_numeric_redefined_as_display(item: &HirDataItem, all_items: &[HirDataItem]) -> bool {
    matches!(item.data_type, HirType::Numeric { .. })
        && all_items.iter().any(|other| {
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

pub(crate) fn emit_single_data_item(
    out: &mut String,
    item: &HirDataItem,
    all_items: &[HirDataItem],
    duplicate_member_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
    fd_max_size: Option<u32>,
) {
    let c_name = sanitize_name(&item.name);
    if item.is_external {
        let should_emit = GLOBAL_EMITTED_EXTERNAL_ITEMS
            .with(|registry| registry.borrow_mut().insert(c_name.clone()));
        if !should_emit {
            return;
        }
    }

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
        let c_type = if item.sign.is_some_and(|sign| sign.separate) {
            "char"
        } else {
            c_type_for_hir_type(&item.data_type)
        };
        match &item.data_type {
            HirType::Alphanumeric { .. } | HirType::National { .. } => {
                // Array types: cast to pointer (acts as array base for memset/strncpy)
                if item.name != "FILLER" && item.name != "PIC" {
                    out.push_str(&format!(
                        "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                    ));
                }
            }
            HirType::Group { members, .. } => {
                // Group REDEFINES: reinterpret as the group's struct type
                emit_group_typedefs(out, &c_name, members, emitted_typedefs, true);
                let td = group_typedef_name_for_layout(&c_name, members, true);
                if item.name != "FILLER" && item.name != "PIC" {
                    out.push_str(&format!(
                        "#define {c_name} (*({td}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                    ));
                }
                // Emit #define macros for children of this REDEFINES group
                // Note: REDEFINES group is a struct, not a union, so no .members wrapper
                emit_group_macros(
                    out,
                    members,
                    std::slice::from_ref(&c_name),
                    &format!("(*({td}*)&{c_redef})"),
                    duplicate_member_names,
                );
                // Emit nested REDEFINES within this top-level REDEFINES group
                emit_group_redefines(
                    out,
                    members,
                    std::slice::from_ref(&c_name),
                    &format!("(*({td}*)&{c_redef})"),
                    duplicate_member_names,
                    emitted_typedefs,
                    true,
                );
            }
            _ => {
                if item.occurs.is_some() {
                    // REDEFINES + OCCURS: cast to pointer so it acts as an array base
                    let c_type = c_type_for_redefines_occurs_item(item);
                    if item.name != "FILLER" && item.name != "PIC" {
                        out.push_str(&format!(
                            "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} OCCURS */\n"
                        ));
                    }
                } else {
                    // Scalar types: dereference cast for lvalue semantics
                    if item.name != "FILLER" && item.name != "PIC" {
                        out.push_str(&format!(
                            "#define {c_name} (*({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                        ));
                    }
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
            // When this record is the primary record of an FD with multiple
            // 01-level records, ensure the buffer is large enough for the
            // largest record to prevent buffer overflow.
            let effective_size = if let Some(max) = fd_max_size {
                std::cmp::max(*size, max) + 1
            } else {
                size + 1
            };
            if item.occurs.is_some() {
                out.push_str(&format!(
                    "static char {c_name}{array_suffix}[{effective_size}];\n",
                ));
            } else {
                out.push_str(&format!("static char {c_name}[{effective_size}];\n",));
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
        _ if item.is_numeric_edited => {
            let storage_size = data_item_storage_size(item);
            out.push_str(&format!(
                "static char {c_name}{array_suffix}[{storage_size}];\n"
            ));
        }
        HirType::Numeric { size, .. }
            if item.sign.is_some_and(|sign| sign.separate)
                || top_level_numeric_redefined_as_display(item, all_items) =>
        {
            let storage_size = if item.sign.is_some_and(|sign| sign.separate) {
                size + 1
            } else {
                *size
            };
            out.push_str(&format!(
                "static char {c_name}{array_suffix}[{}];\n",
                storage_size
            ));
        }
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("static CobolDecimal {c_name}{array_suffix};\n"));
        }
        HirType::Numeric { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Group { members, .. } => {
            // Emit group as union of struct + byte array for group-level operations
            let raw_display_layout = group_needs_raw_display_layout(item, all_items);
            emit_group_typedefs(out, &c_name, members, emitted_typedefs, raw_display_layout);
            let td = group_typedef_name_for_layout(&c_name, members, raw_display_layout);
            out.push_str("static union {\n");
            out.push_str(&format!("    {td} members;\n"));
            out.push_str(&format!("    uint8_t _bytes[sizeof({td})];\n"));
            // When this record is the primary record of an FD with multiple
            // 01-level records, ensure the union is large enough for the
            // largest record to prevent buffer overflow.
            if let Some(max_size) = fd_max_size {
                out.push_str(&format!("    uint8_t _fd_pad[{max_size}];\n"));
            }
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
                std::slice::from_ref(&c_name),
                &format!("{c_name}.members"),
                duplicate_member_names,
                emitted_typedefs,
                raw_display_layout,
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
    raw_display_layout: bool,
) {
    // First, recurse into nested groups using the same duplicate-name
    // disambiguation as the enclosing struct members.
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        let member_base = sanitize_name(&member.name);
        let count = member_name_counts.entry(member_base.clone()).or_insert(0);
        *count += 1;
        let member_c_name = if *count > 1 {
            format!("{}_{}", member_base, count)
        } else {
            member_base
        };
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            emit_group_typedefs(
                out,
                &member_c_name,
                sub_members,
                emitted_typedefs,
                raw_display_layout || group_members_need_raw_display_layout(sub_members),
            );
        }
    }
    // Skip if this exact typedef (name + member layout) has already been emitted
    let typedef_name = group_typedef_name_for_layout(c_name, members, raw_display_layout);
    if !emitted_typedefs.insert(typedef_name.clone()) {
        return;
    }
    let should_emit = GLOBAL_EMITTED_GROUP_TYPEDEFS
        .with(|registry| registry.borrow_mut().insert(typedef_name.clone()));
    if !should_emit {
        return;
    }
    // Emit this level's struct typedef
    out.push_str("typedef struct {\n");
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if member.redefines.is_some() || member.renames.is_some() {
            continue; // REDEFINES handled separately
        }
        emit_group_struct_member(out, member, &mut member_name_counts, raw_display_layout);
    }
    out.push_str(&format!("}} {typedef_name};\n"));
}

/// Emit a single member within a group struct typedef.
pub(crate) fn emit_group_struct_member(
    out: &mut String,
    member: &HirDataItem,
    member_name_counts: &mut HashMap<String, u32>,
    raw_display_layout: bool,
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
        HirType::Numeric { size, .. } if raw_display_layout => {
            // USAGE DISPLAY numeric in group: store as zoned decimal (char[])
            // when REDEFINES/record aliases need byte-for-byte overlay layout.
            let disp_size = if member.sign.is_some_and(|sign| sign.separate) {
                (*size + 1) as usize
            } else {
                *size as usize
            };
            out.push_str(&format!(
                "    char _m_{c_name}{array_suffix}[{disp_size}];\n"
            ));
        }
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("    CobolDecimal _m_{c_name}{array_suffix};\n"));
        }
        HirType::Numeric { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Group {
            members: ref sub_members,
            ..
        } => {
            let child_raw_display_layout =
                raw_display_layout || group_members_need_raw_display_layout(sub_members);
            let td = group_typedef_name_for_layout(&c_name, sub_members, child_raw_display_layout);
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

fn emit_fully_qualified_macro(
    out: &mut String,
    qualifier_names: &[String],
    c_name: &str,
    expr: &str,
) {
    if qualifier_names.is_empty() {
        return;
    }
    let chain = qualifier_names.join("__");
    out.push_str(&format!("#define {chain}__{c_name} {expr}\n"));
}

/// Emit #define macros for all elementary members in a group.
pub(crate) fn emit_group_macros(
    out: &mut String,
    members: &[HirDataItem],
    qualifier_names: &[String],
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
) {
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if let Some((from, thru)) = &member.renames {
            let c_name = sanitize_name(&member.name);
            let c_from = sanitize_name(from);
            let alias_expr = match find_group_member_by_sanitized_name(&c_from, members) {
                Some(HirDataItem {
                    data_type: HirType::Group { .. },
                    ..
                }) => format!("((char*)&{c_from})"),
                Some(_) if thru.is_some() => format!("((char*)&{c_from})"),
                Some(_) => c_from.clone(),
                None if thru.is_some() => format!("((char*)&{c_from})"),
                None => c_from.clone(),
            };
            if member.name != "FILLER" && member.name != "PIC" && !duplicate_names.contains(&c_name)
            {
                out.push_str(&format!(
                    "#define {c_name} {alias_expr} /* RENAMES {c_from} */\n"
                ));
            }
            emit_fully_qualified_macro(
                out,
                qualifier_names,
                &c_name,
                &format!("{alias_expr} /* RENAMES {c_from} */"),
            );
            continue;
        }
        if member.redefines.is_some() {
            continue;
        }
        let base_c_name = sanitize_name(&member.name);
        let count = member_name_counts.entry(base_c_name.clone()).or_insert(0);
        *count += 1;
        let c_name = if *count > 1 {
            format!("{}_{}", base_c_name, count)
        } else {
            base_c_name
        };
        let access_path = format!("{path_prefix}._m_{c_name}");
        if member.name != "FILLER" && member.name != "PIC" && !duplicate_names.contains(&c_name) {
            out.push_str(&format!("#define {c_name} {access_path}\n"));
        }
        // Qualified names in codegen are always emitted as the full chain used
        // by expr::data_name_to_c_name (for example WS_DST__ITEM_GRP__FIELD_A).
        // Partial aliases like ITEM_GRP__FIELD_A are ambiguous across distinct
        // parent records and can cause duplicate macro definitions.
        emit_fully_qualified_macro(out, qualifier_names, &c_name, &access_path);
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
            if member.name != "PIC" {
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
    qualifier_names: &[String],
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
    raw_display_layout: bool,
) {
    for member in members {
        if member.renames.is_some() {
            continue;
        }
        if let Some(ref redef_name) = member.redefines {
            let c_name = sanitize_name(&member.name);
            let c_redef = sanitize_name(redef_name);
            let c_type =
                if raw_display_layout && matches!(member.data_type, HirType::Numeric { .. }) {
                    "char"
                } else if member.sign.is_some_and(|sign| sign.separate) {
                    "char"
                } else {
                    c_type_for_hir_type(&member.data_type)
                };
            let qualified_target = format!("{path_prefix}._m_{c_redef}");
            let emit_aliases = member.name != "FILLER" && member.name != "PIC";
            match &member.data_type {
                HirType::Alphanumeric { .. } | HirType::National { .. } => {
                    let alias_expr =
                        format!("(({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */");
                    if emit_aliases && !duplicate_names.contains(&c_name) {
                        out.push_str(&format!("#define {c_name} {alias_expr}\n"));
                    }
                    emit_fully_qualified_macro(out, qualifier_names, &c_name, &alias_expr);
                }
                HirType::Group {
                    members: grp_members,
                    ..
                } => {
                    emit_group_typedefs(out, &c_name, grp_members, emitted_typedefs, true);
                    let td = group_typedef_name_for_layout(&c_name, grp_members, true);
                    let alias_expr =
                        format!("(*({td}*)&{qualified_target}) /* REDEFINES {c_redef} */");
                    if emit_aliases && !duplicate_names.contains(&c_name) {
                        out.push_str(&format!("#define {c_name} {alias_expr}\n"));
                    }
                    emit_fully_qualified_macro(out, qualifier_names, &c_name, &alias_expr);
                    let mut child_qualifiers = qualifier_names.to_vec();
                    if member.name != "PIC" {
                        child_qualifiers.push(c_name.clone());
                    }
                    let access_expr = format!("(*({td}*)&{qualified_target})");
                    emit_group_macros(
                        out,
                        grp_members,
                        &child_qualifiers,
                        &access_expr,
                        duplicate_names,
                    );
                    // Recurse into REDEFINES group children to emit nested
                    // REDEFINES macros (e.g. RDF3-5-1 REDEFINES RDF3-5).
                    emit_group_redefines(
                        out,
                        grp_members,
                        &child_qualifiers,
                        &access_expr,
                        duplicate_names,
                        emitted_typedefs,
                        true,
                    );
                }
                _ => {
                    if raw_display_layout && matches!(member.data_type, HirType::Numeric { .. }) {
                        let storage_size = data_item_storage_size(member);
                        let alias_expr = format!(
                            "(*(({c_type} (*)[{storage_size}])&{qualified_target})) /* REDEFINES {c_redef} */"
                        );
                        if emit_aliases && !duplicate_names.contains(&c_name) {
                            out.push_str(&format!("#define {c_name} {alias_expr}\n"));
                        }
                        emit_fully_qualified_macro(out, qualifier_names, &c_name, &alias_expr);
                    } else if member.occurs.is_some() {
                        // REDEFINES + OCCURS: pointer cast (acts as array base)
                        let c_type = c_type_for_redefines_occurs_item(member);
                        let alias_expr = format!(
                            "(({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} OCCURS */"
                        );
                        if emit_aliases && !duplicate_names.contains(&c_name) {
                            out.push_str(&format!("#define {c_name} {alias_expr}\n"));
                        }
                        emit_fully_qualified_macro(out, qualifier_names, &c_name, &alias_expr);
                    } else {
                        let alias_expr =
                            format!("(*({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */");
                        if emit_aliases && !duplicate_names.contains(&c_name) {
                            out.push_str(&format!("#define {c_name} {alias_expr}\n"));
                        }
                        emit_fully_qualified_macro(out, qualifier_names, &c_name, &alias_expr);
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
                let mut child_qualifiers = qualifier_names.to_vec();
                if member.name != "PIC" {
                    child_qualifiers.push(c_name.clone());
                }
                emit_group_redefines(
                    out,
                    sub_members,
                    &child_qualifiers,
                    &sub_prefix,
                    duplicate_names,
                    emitted_typedefs,
                    raw_display_layout || group_members_need_raw_display_layout(sub_members),
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

fn c_type_for_redefines_occurs_item(item: &HirDataItem) -> &'static str {
    match item.data_type {
        HirType::Numeric { .. } => "char",
        _ => c_type_for_hir_type(&item.data_type),
    }
}

pub(crate) fn emit_data_init(out: &mut String, items: &[HirDataItem]) {
    emit_data_init_excluding(out, items, &HashSet::new());
}

fn emit_init_store_numeric_display(
    out: &mut String,
    item: &HirDataItem,
    c_name: &str,
    value: &str,
    size: u32,
) {
    if let Some(sign) = item.sign {
        if sign.separate {
            let position = match sign.position {
                HirSignPosition::Leading => 0,
                HirSignPosition::Trailing => 1,
            };
            let storage_size = size + 1;
            out.push_str(&format!(
                "    cobol_store_numeric_display_separate_sign({value}, \
                 (uint8_t*)&({c_name}), {storage_size}, {position});\n"
            ));
        } else if matches!(sign.position, HirSignPosition::Leading) {
            out.push_str(&format!(
                "    cobol_store_numeric_display_leading_sign({value}, \
                 (uint8_t*)&({c_name}), {size});\n"
            ));
        } else {
            out.push_str(&format!(
                "    cobol_store_numeric_display({value}, \
                 (uint8_t*)&({c_name}), {size});\n"
            ));
        }
    } else {
        out.push_str(&format!(
            "    cobol_store_numeric_display({value}, \
             (uint8_t*)&({c_name}), {size});\n"
        ));
    }
}

pub(crate) fn emit_data_init_excluding(
    out: &mut String,
    items: &[HirDataItem],
    excluded_items: &HashSet<String>,
) {
    // Skip top-level items that are already members of a group
    // (they are initialized through the group's recursive init)
    let group_member_names = collect_group_member_names(items);
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue;
        }
        if excluded_items.contains(&c_name) {
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
    if item.renames.is_some() {
        return;
    }
    if let HirType::Group { members, .. } = &item.data_type {
        if let Some(init) = &item.initial_value {
            let (fill, literal): (char, Option<String>) = match init {
                HirLiteral::String(s) => (' ', Some(s.to_string())),
                HirLiteral::Integer(n) => (' ', Some(n.to_string())),
                HirLiteral::Zero => ('0', None),
                HirLiteral::Space => (' ', None),
                _ => ('\0', None),
            };
            if fill == '\0' {
                out.push_str(&format!("    memset(&{c_name}, 0, sizeof({c_name}));\n"));
            } else {
                out.push_str(&format!(
                    "    memset(&{c_name}, '{fill}', sizeof({c_name}));\n"
                ));
            }
            if let Some(literal) = literal {
                let escaped = escape_c_string(&literal);
                out.push_str(&format!(
                    "    memcpy(&{c_name}, \"{escaped}\", sizeof({c_name}) < {} ? sizeof({c_name}) : {});\n",
                    literal.len(),
                    literal.len()
                ));
            }
            return;
        }
        // If this group itself has OCCURS, zero-init the entire array of structs
        // rather than recursing into members (which would fail because we can't
        // access .members on an array element without a subscript).
        if item.occurs.is_some() {
            out.push_str(&format!("    memset(&{c_name}, 0, sizeof({c_name}));\n"));
            let Some(n) = item.occurs else { unreachable!() };
            let depth = group_prefix.map_or(0, |prefix| prefix.matches("[_gi").count());
            let loop_var = format!("_gi{depth}");
            let my_prefix = format!("{c_name}[{loop_var}].members");
            out.push_str(&format!(
                "    for (int {loop_var} = 0; {loop_var} < {n}; {loop_var}++) {{\n"
            ));
            let mut member_name_counts: HashMap<String, u32> = HashMap::new();
            for member in members {
                if member.redefines.is_some() || member.renames.is_some() {
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
            out.push_str("    }\n");
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
            if member.redefines.is_some() || member.renames.is_some() {
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
                if let Some(HirLiteral::String(s)) = &item.initial_value {
                    let escaped = escape_c_string(s);
                    let copy_len = (*size).min(s.len() as u32);
                    if group_prefix.is_some() {
                        out.push_str(&format!(
                            "    for (int _i = 0; _i < {n}; _i++) {{ memset({c_name}[_i], ' ', {size}); memcpy({c_name}[_i], \"{escaped}\", {copy_len}); }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "    for (int _i = 0; _i < {n}; _i++) {{ memset({c_name}[_i], ' ', {size}); memcpy({c_name}[_i], \"{escaped}\", {copy_len}); {c_name}[_i][{size}] = '\\0'; }}\n"
                        ));
                    }
                } else if group_prefix.is_some() {
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
    let in_group = group_prefix.is_some();
    // CobolDecimal initialization. DISPLAY numeric group members are byte
    // fields, including items with implied decimal places.
    if needs_decimal(&item.data_type)
        && !with_active_context(|ctx| ctx.has_display_numeric(&base_c_name))
    {
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
                    let scaled_value = if decimal_places > scale {
                        value * 10_i64.pow(decimal_places - scale)
                    } else if decimal_places < scale {
                        value / 10_i64.pow(scale - decimal_places)
                    } else {
                        value
                    };
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = {scaled_value}, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
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
    if let Some(init) = &item.initial_value {
        match (&item.data_type, init) {
            (HirType::Alphanumeric { size }, HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                let copy_len = (*size).min(s.len() as u32);
                if in_group {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    memcpy({c_name}, \"{escaped}\", {copy_len});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    memcpy({c_name}, \"{escaped}\", {copy_len});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::AllChar(s)) => {
                let pattern = if s.is_empty() { " " } else { s.as_str() };
                let escaped = escape_c_string(pattern);
                let pattern_len = pattern.len();
                if in_group {
                    out.push_str(&format!(
                        "    {{ const char* _all = \"{escaped}\"; for (uint32_t _i = 0; _i < {size}; _i++) {c_name}[_i] = _all[_i % {pattern_len}]; }}\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    {{ const char* _all = \"{escaped}\"; for (uint32_t _i = 0; _i < {size}; _i++) {c_name}[_i] = _all[_i % {pattern_len}]; }}\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Space) => {
                if in_group {
                    out.push_str(&format!("    memset({c_name}, ' ', {size});\n"));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Zero) => {
                if in_group {
                    out.push_str(&format!("    memset({c_name}, '0', {size});\n"));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, '0', {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::HighValue) => {
                if in_group {
                    out.push_str(&format!("    memset({c_name}, 0xFF, {size});\n"));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, 0xFF, {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::LowValue) => {
                if in_group {
                    out.push_str(&format!("    memset({c_name}, 0x00, {size});\n"));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, 0x00, {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Quote) => {
                if in_group {
                    out.push_str(&format!("    memset({c_name}, '\"', {size});\n"));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, '\"', {size});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (HirType::Alphanumeric { size }, HirLiteral::Integer(n)) => {
                let digits_raw = n.to_string();
                let digits = escape_c_string(&digits_raw);
                let copy_len = (*size).min(digits_raw.len() as u32);
                if in_group {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    memcpy({c_name}, \"{digits}\", {copy_len});\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "    memset({c_name}, ' ', {size});\n    memcpy({c_name}, \"{digits}\", {copy_len});\n    {c_name}[{size}] = '\\0';\n"
                    ));
                }
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places,
                    ..
                },
                HirLiteral::Integer(n),
            ) if group_prefix.is_some()
                || c_name.contains("__")
                || c_name.contains("._m_")
                || with_active_context(|ctx| ctx.has_display_numeric(&c_name)) =>
            {
                let value = if *decimal_places > 0 {
                    format!("({n} * {})", 10_i64.pow(*decimal_places))
                } else {
                    n.to_string()
                };
                let value = if item.scale_adjustment > 0 {
                    apply_scale_adjustment_to_store(&value, item.scale_adjustment)
                } else {
                    value
                };
                emit_init_store_numeric_display(out, item, &c_name, &value, *size);
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places,
                    ..
                },
                HirLiteral::String(s),
            ) if group_prefix.is_some()
                || c_name.contains("__")
                || c_name.contains("._m_")
                || with_active_context(|ctx| ctx.has_display_numeric(&c_name)) =>
            {
                let value = s.trim().parse::<i64>().unwrap_or(0);
                let mut value = if *decimal_places > 0 {
                    format!("({value} * {})", 10_i64.pow(*decimal_places))
                } else {
                    value.to_string()
                };
                if item.scale_adjustment > 0 {
                    value = apply_scale_adjustment_to_store(&value, item.scale_adjustment);
                }
                emit_init_store_numeric_display(out, item, &c_name, &value, *size);
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places,
                    ..
                },
                HirLiteral::Decimal(d),
            ) if group_prefix.is_some()
                || c_name.contains("__")
                || c_name.contains("._m_")
                || with_active_context(|ctx| ctx.has_display_numeric(&c_name)) =>
            {
                let (scaled, scale) = parse_decimal_literal(d);
                let mut value = scaled.to_string();
                if *decimal_places > scale {
                    value = format!("(({value}) * {})", 10_i64.pow(*decimal_places - scale));
                } else if scale > *decimal_places {
                    value = format!("(({value}) / {})", 10_i64.pow(scale - *decimal_places));
                }
                if item.scale_adjustment > 0 {
                    value = apply_scale_adjustment_to_store(&value, item.scale_adjustment);
                }
                emit_init_store_numeric_display(out, item, &c_name, &value, *size);
            }
            (
                HirType::Numeric {
                    size,
                    decimal_places: _,
                    ..
                },
                HirLiteral::Zero,
            ) if group_prefix.is_some()
                || c_name.contains("__")
                || c_name.contains("._m_")
                || with_active_context(|ctx| ctx.has_display_numeric(&c_name)) =>
            {
                emit_init_store_numeric_display(out, item, &c_name, "0", *size);
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
                let value = if item.scale_adjustment != 0 {
                    apply_scale_adjustment_to_store(&n.to_string(), item.scale_adjustment)
                } else {
                    n.to_string()
                };
                out.push_str(&format!("    {c_name} = {value};\n"));
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
            (
                HirType::Numeric {
                    decimal_places: 0, ..
                }
                | HirType::Binary { .. },
                HirLiteral::Decimal(d),
            ) => {
                let (scaled, scale) = parse_decimal_literal(d);
                let mut value = scaled.to_string();
                if scale > 0 {
                    value = format!("(({value}) / {})", 10_i64.pow(scale));
                }
                if item.scale_adjustment != 0 {
                    value = apply_scale_adjustment_to_store(&value, item.scale_adjustment);
                }
                out.push_str(&format!("    {c_name} = {value};\n"));
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
                if let HirType::Numeric { size, .. } = item.data_type {
                    if item.sign.is_some_and(|sign| sign.separate)
                        || with_active_context(|ctx| ctx.has_display_numeric(&c_name))
                    {
                        emit_init_store_numeric_display(out, item, &c_name, "0", size);
                    } else {
                        emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
                    }
                } else {
                    emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
                }
            }
        }
    } else {
        if let HirType::Numeric { size, .. } = item.data_type {
            if item.sign.is_some_and(|sign| sign.separate)
                || with_active_context(|ctx| ctx.has_display_numeric(&c_name))
            {
                emit_init_store_numeric_display(out, item, &c_name, "0", size);
            } else {
                emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
            }
        } else {
            emit_default_init(out, &item.data_type, &c_name, group_prefix.is_some());
        }
    }
}

pub(crate) fn emit_default_init(
    out: &mut String,
    data_type: &HirType,
    c_name: &str,
    in_group: bool,
) {
    match data_type {
        HirType::Alphanumeric { size } => {
            if in_group {
                out.push_str(&format!("    memset({c_name}, ' ', {size});\n"));
            } else {
                out.push_str(&format!(
                    "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
                ));
            }
        }
        HirType::Numeric {
            size,
            decimal_places: _,
            ..
        } if in_group
            || c_name.contains("__")
            || c_name.contains("._m_")
            || with_active_context(|ctx| ctx.has_display_numeric(c_name)) =>
        {
            out.push_str(&format!(
                "    cobol_store_numeric_display(0, (uint8_t*)&({c_name}), {size});\n"
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

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::Span;
    use smol_str::SmolStr;

    #[test]
    fn test_emit_fd_alias_macros_uses_primary_bytes_for_scalar_fd_alias() {
        let items = vec![HirDataItem::new(
            "SHORT-OUT",
            HirType::Group {
                members: vec![HirDataItem::new(
                    "FIELD-1",
                    HirType::Alphanumeric { size: 10 },
                    Span::dummy(),
                )],
                size: 10,
            },
            Span::dummy(),
        )];
        let aliases = std::collections::HashMap::from([(
            SmolStr::new("MEDIUM-OUT"),
            SmolStr::new("SHORT-OUT"),
        )]);
        let mut out = String::new();

        emit_fd_alias_macros(&mut out, &items, &aliases);

        assert!(
            out.contains("#define MEDIUM_OUT ((char*)SHORT_OUT._bytes)"),
            "scalar FD aliases should point at the primary record bytes: {out}"
        );
    }

    #[test]
    fn test_emit_fd_alias_macros_uses_primary_bytes_when_alias_item_is_scalar() {
        let items = vec![
            HirDataItem::new(
                "SHORT-OUT",
                HirType::Group {
                    members: vec![HirDataItem::new(
                        "FIELD-1",
                        HirType::Alphanumeric { size: 10 },
                        Span::dummy(),
                    )],
                    size: 10,
                },
                Span::dummy(),
            ),
            HirDataItem::new(
                "MEDIUM-OUT",
                HirType::Alphanumeric { size: 10 },
                Span::dummy(),
            ),
        ];
        let aliases = std::collections::HashMap::from([(
            SmolStr::new("MEDIUM-OUT"),
            SmolStr::new("SHORT-OUT"),
        )]);
        let mut out = String::new();

        emit_fd_alias_macros(&mut out, &items, &aliases);

        assert!(
            out.contains("#define MEDIUM_OUT ((char*)SHORT_OUT._bytes)"),
            "scalar alias items should still point at the primary record bytes: {out}"
        );
    }
}
