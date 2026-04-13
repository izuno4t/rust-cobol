use super::*;
use cobol_hir::HirParagraphId;
use std::cell::{Cell, RefCell};

pub(crate) type FileStatusMap = HashMap<String, String>;
pub(crate) type FileRecordMap = HashMap<String, String>;

#[derive(Debug, Clone)]
pub(crate) struct CommunicationBinding {
    pub(crate) symbolic_queue: Option<String>,
    pub(crate) symbolic_sub_queue_1: Option<String>,
    pub(crate) symbolic_sub_queue_2: Option<String>,
    pub(crate) symbolic_sub_queue_3: Option<String>,
    pub(crate) status_key: Option<String>,
    pub(crate) message_count: Option<String>,
    pub(crate) text_length: Option<String>,
    pub(crate) end_key: Option<String>,
    pub(crate) error_key: Option<String>,
    pub(crate) symbolic_source: Option<String>,
    pub(crate) destination_count: Option<String>,
    pub(crate) destination: Option<String>,
    pub(crate) destination_table_count: Option<u32>,
}

/// Describes the path segments from a top-level group root to a data item,
/// recording which segments carry an OCCURS dimension.
#[derive(Debug, Clone)]
pub(crate) struct SubscriptPathInfo {
    pub(crate) segments: Vec<(String, bool)>,
    pub(crate) root: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AlterableParagraphInfo {
    pub(crate) dispatch_var: String,
    pub(crate) default_target: HirTransferTarget,
    pub(crate) targets: Vec<HirTransferTarget>,
}

/// Shared code-generation context built per HIR program.
///
/// Static lookups are precomputed once, while mutable emission state is kept
/// behind interior mutability so helper functions can accept `&CodegenContext`.
pub(crate) struct CodegenContext {
    subscript_paths: HashMap<String, SubscriptPathInfo>,
    file_record_map: FileRecordMap,
    communication_map: HashMap<String, CommunicationBinding>,
    nested_program_names: HashSet<String>,
    is_subprogram: bool,
    decimal_names: HashSet<String>,
    group_names: HashSet<String>,
    alpha_names: HashSet<String>,
    display_numeric_sizes: HashMap<String, u32>,
    group_alpha_names: HashSet<String>,
    justified_names: HashSet<String>,
    data_item_size_cache: HashMap<String, u32>,
    /// For each primary FD record, the max byte size across all 01-level records
    /// sharing the same FD.  Used by `find_record_len` to return the correct
    /// buffer size for file I/O operations.
    fd_max_record_sizes: HashMap<String, u32>,
    in_body_context: Cell<bool>,
    in_debug_declarative: Cell<bool>,
    goto_label_map: RefCell<HashMap<HirParagraphId, usize>>,
    body_goto_label_map: RefCell<HashMap<HirParagraphId, usize>>,
    perform_thru_counter: Cell<usize>,
    emitted_labels: RefCell<HashSet<String>>,
    alterable_paragraphs: HashMap<HirParagraphId, AlterableParagraphInfo>,
}

thread_local! {
    static ACTIVE_CONTEXT_STACK: RefCell<Vec<*const CodegenContext>> = const { RefCell::new(Vec::new()) };
}

impl CodegenContext {
    pub(crate) fn from_program(program: &HirProgram) -> Self {
        let mut ctx = Self::new(
            &program.data_items,
            &program.file_records,
            &program.communication_descriptions,
            &program.fd_record_aliases,
        );
        ctx.nested_program_names.extend(
            program
                .nested_programs
                .iter()
                .map(|nested| sanitize_name(&nested.name)),
        );
        ctx.alterable_paragraphs = collect_alterable_paragraphs(program);
        ctx
    }

    pub(crate) fn merged_with_program(parent: &CodegenContext, program: &HirProgram) -> Self {
        let mut subscript_paths = parent.subscript_paths.clone();
        subscript_paths.extend(build_subscript_paths(&program.data_items));

        let mut file_record_map = parent.file_record_map.clone();
        file_record_map.extend(
            program
                .file_records
                .iter()
                .map(|(f, r)| (sanitize_name(f), sanitize_name(r))),
        );

        let mut communication_map = parent.communication_map.clone();
        communication_map.extend(build_communication_map(&program.communication_descriptions));

        let mut nested_program_names = parent.nested_program_names.clone();
        nested_program_names.extend(
            program
                .nested_programs
                .iter()
                .map(|nested| sanitize_name(&nested.name)),
        );

        let mut decimal_names = parent.decimal_names.clone();
        decimal_names.extend(build_decimal_names(&program.data_items));

        let mut group_names = parent.group_names.clone();
        group_names.extend(build_group_names(&program.data_items));

        let mut alpha_names = parent.alpha_names.clone();
        alpha_names.extend(build_alpha_names(&program.data_items));

        let mut display_numeric_sizes = parent.display_numeric_sizes.clone();
        display_numeric_sizes.extend(build_display_numeric_sizes(&program.data_items));

        let mut group_alpha_names = parent.group_alpha_names.clone();
        group_alpha_names.extend(build_group_alpha_names(&program.data_items));

        let mut justified_names = parent.justified_names.clone();
        justified_names.extend(build_justified_names(&program.data_items));

        let mut data_item_size_cache = parent.data_item_size_cache.clone();
        data_item_size_cache.extend(build_data_item_size_cache(&program.data_items));

        let mut fd_max_record_sizes = parent.fd_max_record_sizes.clone();
        fd_max_record_sizes.extend(build_fd_max_record_sizes(
            &program.data_items,
            &program.fd_record_aliases,
        ));

        Self {
            subscript_paths,
            file_record_map,
            communication_map,
            nested_program_names,
            is_subprogram: true,
            decimal_names,
            group_names,
            alpha_names,
            display_numeric_sizes,
            group_alpha_names,
            justified_names,
            data_item_size_cache,
            fd_max_record_sizes,
            in_body_context: Cell::new(false),
            in_debug_declarative: Cell::new(false),
            goto_label_map: RefCell::new(HashMap::new()),
            body_goto_label_map: RefCell::new(HashMap::new()),
            perform_thru_counter: Cell::new(0),
            emitted_labels: RefCell::new(HashSet::new()),
            alterable_paragraphs: collect_alterable_paragraphs(program),
        }
    }

    pub(crate) fn new(
        data_items: &[HirDataItem],
        file_records: &HashMap<smol_str::SmolStr, smol_str::SmolStr>,
        communication_descriptions: &[cobol_hir::HirCommunicationDescription],
        fd_record_aliases: &HashMap<smol_str::SmolStr, smol_str::SmolStr>,
    ) -> Self {
        Self {
            subscript_paths: build_subscript_paths(data_items),
            file_record_map: file_records
                .iter()
                .map(|(f, r)| (sanitize_name(f), sanitize_name(r)))
                .collect(),
            communication_map: build_communication_map(communication_descriptions),
            nested_program_names: HashSet::new(),
            is_subprogram: false,
            decimal_names: build_decimal_names(data_items),
            group_names: build_group_names(data_items),
            alpha_names: build_alpha_names(data_items),
            display_numeric_sizes: build_display_numeric_sizes(data_items),
            group_alpha_names: build_group_alpha_names(data_items),
            justified_names: build_justified_names(data_items),
            data_item_size_cache: build_data_item_size_cache(data_items),
            fd_max_record_sizes: build_fd_max_record_sizes(data_items, fd_record_aliases),
            in_body_context: Cell::new(false),
            in_debug_declarative: Cell::new(false),
            goto_label_map: RefCell::new(HashMap::new()),
            body_goto_label_map: RefCell::new(HashMap::new()),
            perform_thru_counter: Cell::new(0),
            emitted_labels: RefCell::new(HashSet::new()),
            alterable_paragraphs: HashMap::new(),
        }
    }

    pub(crate) fn set_in_body_context(&self, value: bool) {
        self.in_body_context.set(value);
    }

    pub(crate) fn in_body_context(&self) -> bool {
        self.in_body_context.get()
    }

    pub(crate) fn set_in_debug_declarative(&self, value: bool) {
        self.in_debug_declarative.set(value);
    }

    pub(crate) fn in_debug_declarative(&self) -> bool {
        self.in_debug_declarative.get()
    }

    pub(crate) fn set_label_map(&self, map: HashMap<HirParagraphId, usize>) {
        *self.goto_label_map.borrow_mut() = map;
        self.perform_thru_counter.set(0);
        self.emitted_labels.borrow_mut().clear();
    }

    pub(crate) fn set_body_label_map(&self, map: HashMap<HirParagraphId, usize>) {
        *self.body_goto_label_map.borrow_mut() = map;
    }

    pub(crate) fn label_id(&self, id: HirParagraphId) -> Option<usize> {
        self.goto_label_map.borrow().get(&id).copied()
    }

    pub(crate) fn body_label_id(&self, id: HirParagraphId) -> Option<usize> {
        self.body_goto_label_map.borrow().get(&id).copied()
    }

    pub(crate) fn has_labels(&self) -> bool {
        !self.goto_label_map.borrow().is_empty()
    }

    pub(crate) fn next_perform_thru_id(&self) -> usize {
        let next = self.perform_thru_counter.get() + 1;
        self.perform_thru_counter.set(next);
        next
    }

    pub(crate) fn mark_label_emitted(&self, label: String) -> bool {
        self.emitted_labels.borrow_mut().insert(label)
    }

    pub(crate) fn subscript_path(&self, c_name: &str) -> Option<SubscriptPathInfo> {
        self.subscript_paths.get(c_name).cloned()
    }

    pub(crate) fn resolve_file_record(&self, sanitized_file_name: &str) -> String {
        self.file_record_map
            .get(sanitized_file_name)
            .cloned()
            .unwrap_or_else(|| sanitized_file_name.to_string())
    }

    pub(crate) fn is_nested_program_name(&self, sanitized_name: &str) -> bool {
        self.nested_program_names.contains(sanitized_name)
    }

    pub(crate) fn is_subprogram(&self) -> bool {
        self.is_subprogram
    }

    pub(crate) fn communication_binding(
        &self,
        sanitized_name: &str,
    ) -> Option<CommunicationBinding> {
        self.communication_map.get(sanitized_name).cloned()
    }

    pub(crate) fn is_decimal_name(&self, c_name: &str) -> bool {
        self.decimal_names.contains(c_name)
    }

    pub(crate) fn is_group_name(&self, c_name: &str) -> bool {
        self.group_names.contains(c_name)
    }

    pub(crate) fn is_alpha_name(&self, c_name: &str) -> bool {
        self.alpha_names.contains(c_name)
    }

    pub(crate) fn is_justified_name(&self, c_name: &str) -> bool {
        self.justified_names.contains(c_name)
    }

    pub(crate) fn display_numeric_size(&self, c_name: &str) -> Option<u32> {
        self.display_numeric_sizes.get(c_name).copied()
    }

    pub(crate) fn has_display_numeric(&self, c_name: &str) -> bool {
        self.display_numeric_sizes.contains_key(c_name)
    }

    pub(crate) fn is_group_alpha_name(&self, c_name: &str) -> bool {
        self.group_alpha_names.contains(c_name)
    }

    pub(crate) fn data_item_size(&self, c_name: &str) -> Option<u32> {
        self.data_item_size_cache.get(c_name).copied()
    }

    /// Return the max FD record size for a primary record name, if it
    /// exceeds the primary record's own size.
    pub(crate) fn fd_max_record_size(&self, c_name: &str) -> Option<u32> {
        self.fd_max_record_sizes.get(c_name).copied()
    }

    pub(crate) fn alterable_paragraph(&self, id: HirParagraphId) -> Option<AlterableParagraphInfo> {
        self.alterable_paragraphs.get(&id).cloned()
    }

    pub(crate) fn alterable_paragraphs(&self) -> Vec<AlterableParagraphInfo> {
        self.alterable_paragraphs.values().cloned().collect()
    }
}

fn collect_alterable_paragraphs(
    program: &HirProgram,
) -> HashMap<HirParagraphId, AlterableParagraphInfo> {
    let mut altered_targets: HashMap<HirParagraphId, Vec<HirTransferTarget>> = HashMap::new();
    collect_alter_targets_from_block(&program.body, &mut altered_targets);
    for paragraph in &program.paragraphs {
        collect_alter_targets_from_block(&paragraph.body, &mut altered_targets);
    }
    for decl in &program.declaratives {
        collect_alter_targets_from_block(&decl.body, &mut altered_targets);
    }

    let mut paragraphs_by_id = HashMap::new();
    for paragraph in &program.paragraphs {
        paragraphs_by_id.insert(paragraph.id, paragraph);
    }

    let mut out = HashMap::new();
    for (paragraph_id, alter_targets) in altered_targets {
        let Some(paragraph) = paragraphs_by_id.get(&paragraph_id) else {
            continue;
        };
        let Some(default_target) = paragraph.body.iter().find_map(|stmt| match stmt {
            HirStatement::GoTo {
                targets,
                depending_on: None,
                ..
            } if !targets.is_empty() => Some(targets[0].clone()),
            _ => None,
        }) else {
            continue;
        };

        let dispatch_var = format!("_alter_target_{}", sanitize_name(&paragraph.name));
        let mut targets = vec![default_target.clone()];
        for target in alter_targets {
            if !targets.iter().any(|existing| existing == &target) {
                targets.push(target);
            }
        }
        out.insert(
            paragraph_id,
            AlterableParagraphInfo {
                dispatch_var,
                default_target,
                targets,
            },
        );
    }

    out
}

fn collect_alter_targets_from_block(
    stmts: &[HirStatement],
    altered_targets: &mut HashMap<HirParagraphId, Vec<HirTransferTarget>>,
) {
    for stmt in stmts {
        if let HirStatement::Alter { from, to, .. } = stmt {
            if let Some(paragraph_id) = from.paragraph_id() {
                altered_targets
                    .entry(paragraph_id)
                    .or_default()
                    .push(to.clone());
            }
        }
    }
}

fn build_communication_map(
    communication_descriptions: &[cobol_hir::HirCommunicationDescription],
) -> HashMap<String, CommunicationBinding> {
    communication_descriptions
        .iter()
        .map(|cd| {
            (
                sanitize_name(&cd.name),
                CommunicationBinding {
                    symbolic_queue: cd.symbolic_queue.as_ref().map(sanitize_name),
                    symbolic_sub_queue_1: cd.symbolic_sub_queue_1.as_ref().map(sanitize_name),
                    symbolic_sub_queue_2: cd.symbolic_sub_queue_2.as_ref().map(sanitize_name),
                    symbolic_sub_queue_3: cd.symbolic_sub_queue_3.as_ref().map(sanitize_name),
                    status_key: cd.status_key.as_ref().map(sanitize_name),
                    message_count: cd.message_count.as_ref().map(sanitize_name),
                    text_length: cd.text_length.as_ref().map(sanitize_name),
                    end_key: cd.end_key.as_ref().map(sanitize_name),
                    error_key: cd.error_key.as_ref().map(sanitize_name),
                    symbolic_source: cd.symbolic_source.as_ref().map(sanitize_name),
                    destination_count: cd.destination_count.as_ref().map(sanitize_name),
                    destination: cd.destination.as_ref().map(sanitize_name),
                    destination_table_count: cd.destination_table_count,
                },
            )
        })
        .collect()
}

fn build_alpha_names(items: &[HirDataItem]) -> HashSet<String> {
    fn collect(items: &[HirDataItem], acc: &mut HashSet<String>) {
        for item in items {
            if matches!(item.data_type, HirType::Alphanumeric { .. }) {
                acc.insert(sanitize_name(&item.name));
            }
            if let HirType::Group { members, .. } = &item.data_type {
                collect(members, acc);
            }
        }
    }

    let mut names = HashSet::new();
    collect(items, &mut names);
    names
}

pub(crate) fn with_pushed_context<R>(ctx: &CodegenContext, f: impl FnOnce() -> R) -> R {
    ACTIVE_CONTEXT_STACK.with(|stack| stack.borrow_mut().push(ctx as *const CodegenContext));
    let result = f();
    ACTIVE_CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}

pub(crate) fn with_active_context<R>(f: impl FnOnce(&CodegenContext) -> R) -> R {
    ACTIVE_CONTEXT_STACK.with(|stack| {
        let ptr = {
            let borrow = stack.borrow();
            *borrow.last().expect("active codegen context is not set")
        };
        // SAFETY: The pointer refers to the top entry of the thread-local
        // context stack. Callers only access it synchronously during codegen.
        unsafe { f(&*ptr) }
    })
}

pub(crate) fn build_subscript_paths(
    data_items: &[HirDataItem],
) -> HashMap<String, SubscriptPathInfo> {
    let mut map = HashMap::new();
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            let root = sanitize_name(&item.name);
            let root_has_occurs = item.occurs.is_some();
            let root_is_redefines = item.redefines.is_some();
            collect_subscript_paths(
                &mut map,
                members,
                &root,
                &[],
                &[],
                root_has_occurs,
                root_is_redefines,
            );
        }
    }
    map
}

pub(crate) fn build_decimal_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_decimal_names(&mut set, data_items);
    set
}

pub(crate) fn collect_decimal_names(set: &mut HashSet<String>, data_items: &[HirDataItem]) {
    for item in data_items {
        if needs_decimal(&item.data_type) {
            set.insert(sanitize_name(&item.name));
        }
        if let HirType::Group { members, .. } = &item.data_type {
            collect_decimal_names(set, members);
        }
    }
}

pub(crate) fn build_justified_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_justified_names(&mut set, data_items);
    set
}

fn collect_justified_names(set: &mut HashSet<String>, data_items: &[HirDataItem]) {
    for item in data_items {
        if item.justified {
            set.insert(sanitize_name(&item.name));
        }
        if let HirType::Group { members, .. } = &item.data_type {
            collect_justified_names(set, members);
        }
    }
}

pub(crate) fn build_display_numeric_sizes(data_items: &[HirDataItem]) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            collect_display_numeric_sizes(&mut map, members);
        }
    }
    map
}

pub(crate) fn collect_display_numeric_sizes(
    map: &mut HashMap<String, u32>,
    members: &[HirDataItem],
) {
    for member in members {
        let c_name = sanitize_name(&member.name);
        match &member.data_type {
            HirType::Numeric {
                size,
                decimal_places: 0,
                ..
            } => {
                map.insert(c_name, *size);
            }
            HirType::Group {
                members: sub_members,
                ..
            } => collect_display_numeric_sizes(map, sub_members),
            _ => {}
        }
    }
    // Register RENAMES items (level 66) that alias a single display numeric field.
    // RENAMES with THRU spans multiple fields and is treated as alphanumeric.
    for member in members {
        if let Some((ref from, ref thru)) = member.renames {
            if thru.is_none() {
                let from_c = sanitize_name(from);
                if let Some(&size) = map.get(&from_c) {
                    let c_name = sanitize_name(&member.name);
                    map.insert(c_name, size);
                }
            }
        }
    }
}

pub(crate) fn build_group_alpha_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            collect_group_alpha_names(&mut set, members);
        }
    }
    set
}

pub(crate) fn collect_group_alpha_names(set: &mut HashSet<String>, members: &[HirDataItem]) {
    for member in members {
        let c_name = sanitize_name(&member.name);
        match &member.data_type {
            HirType::Alphanumeric { .. } => {
                set.insert(c_name);
            }
            HirType::Group {
                members: sub_members,
                ..
            } => collect_group_alpha_names(set, sub_members),
            _ => {}
        }
    }
}

pub(crate) fn build_group_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_group_names(&mut set, data_items);
    set
}

pub(crate) fn collect_group_names(set: &mut HashSet<String>, data_items: &[HirDataItem]) {
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            set.insert(sanitize_name(&item.name));
            collect_group_names(set, members);
        }
    }
}

pub(crate) fn collect_subscript_paths(
    map: &mut HashMap<String, SubscriptPathInfo>,
    members: &[HirDataItem],
    root: &str,
    ancestor_segments: &[(String, bool)],
    ancestor_names: &[String],
    root_has_occurs: bool,
    parent_is_redefines: bool,
) {
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        let base_c_name = sanitize_name(&member.name);
        let count = member_name_counts.entry(base_c_name.clone()).or_insert(0);
        *count += 1;
        let member_c_name = if *count > 1 {
            format!("{}_{}", base_c_name, count)
        } else {
            base_c_name.clone()
        };
        let member_has_occurs = member.occurs.is_some();
        let segment_suffix = if parent_is_redefines {
            format!("._m_{member_c_name}")
        } else {
            format!(".members._m_{member_c_name}")
        };
        let mut segments: Vec<(String, bool)> = ancestor_segments.to_vec();
        segments.push((segment_suffix, member_has_occurs));
        let mut qualified_names: Vec<String> = ancestor_names.to_vec();
        qualified_names.push(member_c_name.clone());

        let any_occurs = root_has_occurs || segments.iter().any(|(_, has)| *has);
        if any_occurs {
            let new_occurs_count = segments.iter().filter(|(_, has)| *has).count();
            let candidate = SubscriptPathInfo {
                segments: segments.clone(),
                root: root.to_string(),
            };
            let should_insert = match map.get(&base_c_name) {
                Some(existing) => {
                    let existing_count = existing.segments.iter().filter(|(_, has)| *has).count();
                    new_occurs_count > existing_count
                        || (new_occurs_count == existing_count
                            && matches!(
                                member.data_type,
                                HirType::Numeric {
                                    decimal_places: 0,
                                    ..
                                }
                            ))
                }
                None => true,
            };
            if should_insert {
                map.insert(base_c_name.clone(), candidate.clone());
            }
            let qualified_key = std::iter::once(root.to_string())
                .chain(qualified_names.iter().cloned())
                .collect::<Vec<_>>()
                .join("__");
            map.insert(qualified_key, candidate);
        }

        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_subscript_paths(
                map,
                sub_members,
                root,
                &segments,
                &qualified_names,
                root_has_occurs,
                false,
            );
        }
    }
}

pub(crate) fn build_data_item_size_cache(items: &[HirDataItem]) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    populate_size_cache(items, &mut map);
    map
}

pub(crate) fn populate_size_cache(items: &[HirDataItem], map: &mut HashMap<String, u32>) {
    for item in items {
        let c_name = sanitize_name(&item.name);
        let size = data_item_byte_size(&item.data_type);
        map.entry(c_name).or_insert(size);
        if let HirType::Group { members, .. } = &item.data_type {
            populate_size_cache(members, map);
        }
    }
}

/// Build a mapping from primary FD record name → max byte size across all
/// 01-level records in the same FD.  Only entries where an alias record is
/// larger than the primary are included.
fn build_fd_max_record_sizes(
    data_items: &[HirDataItem],
    fd_record_aliases: &HashMap<smol_str::SmolStr, smol_str::SmolStr>,
) -> HashMap<String, u32> {
    if fd_record_aliases.is_empty() {
        return HashMap::new();
    }

    // Build size cache for quick lookup
    let mut size_cache = HashMap::new();
    populate_size_cache(data_items, &mut size_cache);

    // Group aliases by primary record name
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
        let primary_size = size_cache.get(c_primary.as_str()).copied().unwrap_or(80);
        let mut max_size = primary_size;
        for c_alias in aliases {
            let alias_size = size_cache.get(c_alias.as_str()).copied().unwrap_or(0);
            if alias_size > max_size {
                max_size = alias_size;
            }
        }
        if max_size > primary_size {
            result.insert(c_primary.clone(), max_size);
        }
    }
    result
}
