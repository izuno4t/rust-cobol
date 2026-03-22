use super::*;
use std::cell::{Cell, RefCell};

pub(crate) type FileStatusMap = HashMap<String, String>;
pub(crate) type FileRecordMap = HashMap<String, String>;

/// Describes the path segments from a top-level group root to a data item,
/// recording which segments carry an OCCURS dimension.
#[derive(Debug, Clone)]
pub(crate) struct SubscriptPathInfo {
    pub(crate) segments: Vec<(String, bool)>,
    pub(crate) root: String,
}

/// Shared code-generation context built per HIR program.
///
/// Static lookups are precomputed once, while mutable emission state is kept
/// behind interior mutability so helper functions can accept `&CodegenContext`.
pub(crate) struct CodegenContext {
    subscript_paths: HashMap<String, SubscriptPathInfo>,
    file_record_map: FileRecordMap,
    decimal_names: HashSet<String>,
    group_names: HashSet<String>,
    display_numeric_sizes: HashMap<String, u32>,
    group_alpha_names: HashSet<String>,
    data_item_size_cache: HashMap<String, u32>,
    in_body_context: Cell<bool>,
    goto_label_map: RefCell<HashMap<String, usize>>,
    perform_thru_counter: Cell<usize>,
    emitted_labels: RefCell<HashSet<String>>,
}

thread_local! {
    static ACTIVE_CONTEXT_STACK: RefCell<Vec<*const CodegenContext>> = const { RefCell::new(Vec::new()) };
}

impl CodegenContext {
    pub(crate) fn from_program(program: &HirProgram) -> Self {
        Self::new(&program.data_items, &program.file_records)
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

        let mut decimal_names = parent.decimal_names.clone();
        decimal_names.extend(build_decimal_names(&program.data_items));

        let mut group_names = parent.group_names.clone();
        group_names.extend(build_group_names(&program.data_items));

        let mut display_numeric_sizes = parent.display_numeric_sizes.clone();
        display_numeric_sizes.extend(build_display_numeric_sizes(&program.data_items));

        let mut group_alpha_names = parent.group_alpha_names.clone();
        group_alpha_names.extend(build_group_alpha_names(&program.data_items));

        let mut data_item_size_cache = parent.data_item_size_cache.clone();
        data_item_size_cache.extend(build_data_item_size_cache(&program.data_items));

        Self {
            subscript_paths,
            file_record_map,
            decimal_names,
            group_names,
            display_numeric_sizes,
            group_alpha_names,
            data_item_size_cache,
            in_body_context: Cell::new(false),
            goto_label_map: RefCell::new(HashMap::new()),
            perform_thru_counter: Cell::new(0),
            emitted_labels: RefCell::new(HashSet::new()),
        }
    }

    pub(crate) fn new(
        data_items: &[HirDataItem],
        file_records: &HashMap<smol_str::SmolStr, smol_str::SmolStr>,
    ) -> Self {
        Self {
            subscript_paths: build_subscript_paths(data_items),
            file_record_map: file_records
                .iter()
                .map(|(f, r)| (sanitize_name(f), sanitize_name(r)))
                .collect(),
            decimal_names: build_decimal_names(data_items),
            group_names: build_group_names(data_items),
            display_numeric_sizes: build_display_numeric_sizes(data_items),
            group_alpha_names: build_group_alpha_names(data_items),
            data_item_size_cache: build_data_item_size_cache(data_items),
            in_body_context: Cell::new(false),
            goto_label_map: RefCell::new(HashMap::new()),
            perform_thru_counter: Cell::new(0),
            emitted_labels: RefCell::new(HashSet::new()),
        }
    }

    pub(crate) fn set_in_body_context(&self, value: bool) {
        self.in_body_context.set(value);
    }

    pub(crate) fn in_body_context(&self) -> bool {
        self.in_body_context.get()
    }

    pub(crate) fn set_label_map(&self, map: HashMap<String, usize>) {
        *self.goto_label_map.borrow_mut() = map;
        self.perform_thru_counter.set(0);
        self.emitted_labels.borrow_mut().clear();
    }

    pub(crate) fn label_id(&self, name: &str) -> Option<usize> {
        self.goto_label_map.borrow().get(name).copied()
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

    pub(crate) fn is_decimal_name(&self, c_name: &str) -> bool {
        self.decimal_names.contains(c_name)
    }

    pub(crate) fn is_group_name(&self, c_name: &str) -> bool {
        self.group_names.contains(c_name)
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

pub(crate) fn build_subscript_paths(data_items: &[HirDataItem]) -> HashMap<String, SubscriptPathInfo> {
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
    root_has_occurs: bool,
    parent_is_redefines: bool,
) {
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        let c_name = sanitize_name(&member.name);
        let member_has_occurs = member.occurs.is_some();
        let segment_suffix = if parent_is_redefines {
            format!("._m_{c_name}")
        } else {
            format!(".members._m_{c_name}")
        };
        let mut segments: Vec<(String, bool)> = ancestor_segments.to_vec();
        segments.push((segment_suffix, member_has_occurs));

        let any_occurs = root_has_occurs || segments.iter().any(|(_, has)| *has);
        if any_occurs {
            let new_occurs_count = segments.iter().filter(|(_, has)| *has).count();
            let should_insert = match map.get(&c_name) {
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
                map.insert(
                    c_name.clone(),
                    SubscriptPathInfo {
                        segments: segments.clone(),
                        root: root.to_string(),
                    },
                );
            }
        }

        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_subscript_paths(map, sub_members, root, &segments, root_has_occurs, false);
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
