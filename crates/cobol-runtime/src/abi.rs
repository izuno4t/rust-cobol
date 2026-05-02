use std::fmt::Write;

use crate::sort_merge::{
    SORT_KEY_ALPHA, SORT_KEY_DISPLAY_NUMERIC, SORT_KEY_SIGNED_BINARY, SORT_KEY_UNSIGNED_BINARY,
};

/// Emit the C ABI declarations that generated C code relies on.
///
/// This is the single source of truth for the codegen/runtime boundary that is
/// materialized into generated C translation units.
pub fn emit_c_declarations(out: &mut String) {
    out.push_str("/* Runtime library declarations */\n");
    out.push_str("typedef struct CobolDecimal {\n");
    out.push_str("    int64_t value;\n");
    out.push_str("    int32_t scale;\n");
    out.push_str("    int32_t size;\n");
    out.push_str("    _Bool is_signed;\n");
    out.push_str("} CobolDecimal;\n");
    out.push_str("typedef struct CobolStringSource {\n");
    out.push_str("    const uint8_t* ptr;\n");
    out.push_str("    uint32_t len;\n");
    out.push_str("    const uint8_t* delim_ptr;\n");
    out.push_str("    uint32_t delim_len;\n");
    out.push_str("} CobolStringSource;\n");
    out.push_str("typedef struct CobolUnstringTarget {\n");
    out.push_str("    uint8_t* ptr;\n");
    out.push_str("    uint32_t len;\n");
    out.push_str("    uint8_t* delimiter_ptr;\n");
    out.push_str("    uint32_t delimiter_len;\n");
    out.push_str("    uint32_t* count_ptr;\n");
    out.push_str("} CobolUnstringTarget;\n");
    out.push_str("typedef struct SortKey {\n");
    out.push_str("    uint32_t offset;\n");
    out.push_str("    uint32_t length;\n");
    out.push_str("    _Bool ascending;\n");
    out.push_str("    uint8_t key_type;\n");
    out.push_str("} SortKey;\n");
    out.push_str("extern void cobol_display_string(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_display_int(int64_t value);\n");
    out.push_str("extern void cobol_display_newline(void);\n");
    out.push_str("extern void cobol_display_space(void);\n");
    out.push_str("extern void cobol_display_flush(void);\n");
    out.push_str("extern void cobol_stop_run(void) __attribute__((noreturn));\n");
    out.push_str("extern void cobol_goback(void);\n");
    out.push_str("extern void cobol_call_enter(uintptr_t jmp_buf_ptr);\n");
    out.push_str("extern void cobol_call_leave(void);\n");
    out.push_str("/* Communication runtime declarations */\n");
    out.push_str(
        "extern uint32_t cobol_comm_enable(const uint8_t* name_ptr, uint32_t name_len, int32_t mode, int32_t terminal, const uint8_t* key_ptr, uint32_t key_len, const uint8_t* queue_ptr, uint32_t queue_len, const uint8_t* sub1_ptr, uint32_t sub1_len, const uint8_t* sub2_ptr, uint32_t sub2_len, const uint8_t* sub3_ptr, uint32_t sub3_len, const uint8_t* source_ptr, uint32_t source_len, const uint8_t* dest_ptr, uint32_t dest_item_len, uint32_t dest_stride, uint32_t dest_count, uint32_t dest_area_count, uint32_t dest_area_len, uint8_t* error_key_ptr, uint32_t error_key_item_len, uint32_t error_key_stride, uint32_t error_key_count, uint32_t error_key_area_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_comm_disable(const uint8_t* name_ptr, uint32_t name_len, int32_t mode, int32_t terminal, const uint8_t* key_ptr, uint32_t key_len, const uint8_t* queue_ptr, uint32_t queue_len, const uint8_t* sub1_ptr, uint32_t sub1_len, const uint8_t* sub2_ptr, uint32_t sub2_len, const uint8_t* sub3_ptr, uint32_t sub3_len, const uint8_t* source_ptr, uint32_t source_len, const uint8_t* dest_ptr, uint32_t dest_item_len, uint32_t dest_stride, uint32_t dest_count, uint32_t dest_area_count, uint32_t dest_area_len, uint8_t* error_key_ptr, uint32_t error_key_item_len, uint32_t error_key_stride, uint32_t error_key_count, uint32_t error_key_area_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_comm_send(const uint8_t* name_ptr, uint32_t name_len, const uint8_t* from_ptr, uint32_t from_len, uint32_t effective_len, int32_t option_kind, int64_t option_value, int32_t replacing_line, const uint8_t* dest_ptr, uint32_t dest_item_len, uint32_t dest_stride, uint32_t dest_count, uint32_t dest_area_count, uint32_t dest_area_len, uint8_t* error_key_ptr, uint32_t error_key_item_len, uint32_t error_key_stride, uint32_t error_key_count, uint32_t error_key_area_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_comm_receive(const uint8_t* name_ptr, uint32_t name_len, int32_t mode, uint8_t* into_ptr, uint32_t into_len, uint32_t* text_length, const uint8_t* queue_ptr, uint32_t queue_len, const uint8_t* sub1_ptr, uint32_t sub1_len, const uint8_t* sub2_ptr, uint32_t sub2_len, const uint8_t* sub3_ptr, uint32_t sub3_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_comm_last_end_key(const uint8_t* name_ptr, uint32_t name_len);\n",
    );
    out.push_str("extern uint32_t cobol_comm_purge(const uint8_t* name_ptr, uint32_t name_len);\n");
    out.push_str(
        "extern uint32_t cobol_comm_message_count(const uint8_t* name_ptr, uint32_t name_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_comm_accept_count(const uint8_t* name_ptr, uint32_t name_len, uint32_t* count_out, const uint8_t* queue_ptr, uint32_t queue_len, const uint8_t* sub1_ptr, uint32_t sub1_len, const uint8_t* sub2_ptr, uint32_t sub2_len, const uint8_t* sub3_ptr, uint32_t sub3_len);\n",
    );
    out.push_str(
        "extern void cobol_runtime_now_parts(int32_t* year_ptr, int32_t* month_ptr, int32_t* day_ptr, int32_t* yday1_ptr, int32_t* wday_mon1_ptr, int32_t* hour_ptr, int32_t* minute_ptr, int32_t* sec_centis_ptr);\n",
    );
    out.push_str("/* File I/O runtime declarations */\n");
    out.push_str(
        "extern uint32_t cobol_file_open(uint32_t file_id, const uint8_t* path_ptr, uint32_t path_len, uint32_t org, uint32_t access, uint32_t mode, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_open_indexed(uint32_t file_id, const uint8_t* path_ptr, uint32_t path_len, uint32_t access, uint32_t mode, uint32_t record_len, uint32_t key_offset, uint32_t key_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_add_alternate_index(uint32_t file_id, uint32_t key_offset, uint32_t key_len, uint32_t duplicates);\n",
    );
    out.push_str("extern uint32_t cobol_file_close(uint32_t file_id);\n");
    out.push_str(
        "extern uint32_t cobol_file_read_next(uint32_t file_id, uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_read_key(uint32_t file_id, const uint8_t* key_ptr, uint32_t key_len, uint32_t key_offset, uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_write(uint32_t file_id, const uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_rewrite(uint32_t file_id, const uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str("extern uint32_t cobol_file_delete(uint32_t file_id);\n");
    out.push_str("extern uint64_t cobol_file_current_record(uint32_t file_id);\n");
    out.push_str(
        "extern uint32_t cobol_file_start(uint32_t file_id, const uint8_t* key_ptr, uint32_t key_len, uint32_t key_offset, uint32_t mode);\n",
    );
    out.push_str("/* Class condition runtime declarations */\n");
    out.push_str("extern int32_t cobol_is_numeric(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic_lower(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic_upper(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("/* Alphanumeric comparison runtime declaration */\n");
    out.push_str(
        "extern int32_t cobol_compare_alphanumeric(const uint8_t* a, uint32_t a_len, const uint8_t* b, uint32_t b_len);\n",
    );
    out.push_str("/* String operations runtime declarations */\n");
    out.push_str(
        "extern void cobol_move_string(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_move_string_right(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_move_alphanumeric_edited(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len, const uint8_t* pic, uint32_t pic_len);\n",
    );
    out.push_str(
        "extern void cobol_move_numeric_to_display(int64_t value, int32_t scale, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_store_numeric_display(int64_t value, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern int64_t cobol_display_to_int64(const uint8_t* src, uint32_t src_len);\n");
    out.push_str(
        "extern int32_t cobol_string_concat(const CobolStringSource* sources, uint32_t source_count, uint8_t* dst, uint32_t dst_len, uint32_t* pointer);\n",
    );
    out.push_str(
        "extern int32_t cobol_unstring(const uint8_t* src, uint32_t src_len, const uint8_t* delim, uint32_t delim_len, CobolUnstringTarget* targets, uint32_t target_count, uint32_t* pointer, uint32_t* tallying);\n",
    );
    out.push_str(
        "extern uint32_t cobol_inspect_tallying(const uint8_t* src, uint32_t src_len, const uint8_t* search, uint32_t search_len, uint32_t mode);\n",
    );
    out.push_str(
        "extern void cobol_inspect_replacing(uint8_t* src, uint32_t src_len, const uint8_t* search, uint32_t search_len, const uint8_t* replace, uint32_t replace_len, uint32_t mode);\n",
    );
    out.push_str(
        "extern void cobol_inspect_converting(uint8_t* src, uint32_t src_len, const uint8_t* from, uint32_t from_len, const uint8_t* to, uint32_t to_len);\n",
    );
    out.push_str("/* Sort/Merge runtime declarations */\n");
    out.push_str(
        "extern void cobol_sort(uint8_t* records, uint32_t count, uint32_t rec_len, const SortKey* keys, uint32_t key_count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_merge(const uint32_t* inputs, uint32_t input_count, uint32_t output, const SortKey* keys, uint32_t key_count, uint32_t rec_len);\n",
    );
    out.push_str("extern uint32_t cobol_sort_buffer_init(uint32_t record_len);\n");
    out.push_str(
        "extern void cobol_sort_buffer_release(uint32_t buf_id, const uint8_t* record, uint32_t record_len);\n",
    );
    out.push_str(
        "extern void cobol_sort_buffer_sort(uint32_t buf_id, const SortKey* keys, uint32_t key_count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_sort_buffer_return(uint32_t buf_id, uint8_t* record, uint32_t record_len);\n",
    );
    out.push_str("extern void cobol_sort_buffer_free(uint32_t buf_id);\n");
    out.push_str("static uint32_t _sort_buf_id = 0;\n");
    let _ = writeln!(out, "#define SORT_KEY_ALPHA {SORT_KEY_ALPHA}");
    let _ = writeln!(
        out,
        "#define SORT_KEY_SIGNED_BINARY {SORT_KEY_SIGNED_BINARY}"
    );
    let _ = writeln!(
        out,
        "#define SORT_KEY_UNSIGNED_BINARY {SORT_KEY_UNSIGNED_BINARY}"
    );
    let _ = writeln!(
        out,
        "#define SORT_KEY_DISPLAY_NUMERIC {SORT_KEY_DISPLAY_NUMERIC}"
    );
    out.push_str("/* Intrinsic function runtime declarations */\n");
    out.push_str("extern uint32_t cobol_func_current_date(uint8_t* buf, uint32_t buf_len);\n");
    out.push_str("extern uint32_t cobol_func_length(const uint8_t* ptr, uint32_t len);\n");
    out.push_str(
        "extern uint32_t cobol_func_trim(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len, uint32_t mode);\n",
    );
    out.push_str("extern void cobol_func_upper_case(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_func_lower_case(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_func_reverse(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int64_t cobol_func_numval(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern double cobol_func_numval_double(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int64_t cobol_func_max_int(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_min_int(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_mod(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_integer(int64_t value, int32_t scale);\n");
    out.push_str("extern uint32_t cobol_func_ord(uint8_t c);\n");
    out.push_str("extern uint8_t cobol_func_char(uint32_t ord);\n");
    out.push_str("/* Mathematical intrinsic function declarations */\n");
    out.push_str("extern int64_t cobol_func_abs(int64_t value);\n");
    out.push_str("extern double cobol_func_abs_float(double value);\n");
    out.push_str("extern double cobol_func_sqrt(double value);\n");
    out.push_str("extern double cobol_func_exp(double value);\n");
    out.push_str("extern double cobol_func_exp10(double value);\n");
    out.push_str("extern double cobol_func_log(double value);\n");
    out.push_str("extern double cobol_func_log10(double value);\n");
    out.push_str("extern double cobol_func_sin(double value);\n");
    out.push_str("extern double cobol_func_cos(double value);\n");
    out.push_str("extern double cobol_func_tan(double value);\n");
    out.push_str("extern double cobol_func_asin(double value);\n");
    out.push_str("extern double cobol_func_acos(double value);\n");
    out.push_str("extern double cobol_func_atan(double value);\n");
    out.push_str("extern int64_t cobol_func_ceiling(double value);\n");
    out.push_str("extern int64_t cobol_func_floor(double value);\n");
    out.push_str("extern int64_t cobol_func_factorial(int64_t n);\n");
    out.push_str("extern double cobol_func_rem(double a, double b);\n");
    out.push_str("extern double cobol_func_random(int64_t seed);\n");
    out.push_str("extern int64_t cobol_func_sign(int64_t value);\n");
    out.push_str("extern double cobol_func_mean(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_median(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_midrange(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_range(const double* values, int32_t count);\n");
    out.push_str(
        "extern double cobol_func_standard_deviation(const double* values, int32_t count);\n",
    );
    out.push_str("extern double cobol_func_variance(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_sum_float(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_annuity(double rate, int64_t periods);\n");
    out.push_str(
        "extern double cobol_func_present_value(double rate, const double* values, int32_t count);\n",
    );
    out.push_str("/* Date/time intrinsic function declarations */\n");
    out.push_str("extern int64_t cobol_func_integer_of_date(int64_t yyyymmdd);\n");
    out.push_str("extern int64_t cobol_func_date_of_integer(int64_t day_count);\n");
    out.push_str("extern int64_t cobol_func_integer_of_day(int64_t yyyyddd);\n");
    out.push_str("extern int64_t cobol_func_day_of_integer(int64_t day_count);\n");
    out.push_str("extern int64_t cobol_func_date_to_yyyymmdd(int64_t yymmdd, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_year_to_yyyy(int64_t yy, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_day_to_yyyyddd(int64_t yyddd, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_test_date_yyyymmdd(int64_t yyyymmdd);\n");
    out.push_str("extern int64_t cobol_func_test_day_yyyyddd(int64_t yyyyddd);\n");
    out.push_str("extern uint32_t cobol_func_when_compiled(uint8_t* buf, uint32_t buf_len);\n");
    out.push_str("extern int64_t cobol_func_max_int_n(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_min_int_n(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_ord_max(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_ord_min(const int64_t* values, int32_t count);\n");
    out.push_str(
        "extern int32_t cobol_func_max_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int32_t cobol_func_min_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int64_t cobol_func_ord_max_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int64_t cobol_func_ord_min_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_func_stored_char_length(const uint8_t* ptr, uint32_t len);\n",
    );
    out.push_str("/* COBOL 2002+ runtime declarations */\n");
    out.push_str(
        "extern void cobol_raise(const char* exception_name) __attribute__((noreturn));\n",
    );
    out.push_str("extern void cobol_resume(const char* target);\n");
    out.push_str("extern void cobol_exception_push(uintptr_t jmp_buf_ptr);\n");
    out.push_str("extern void cobol_exception_pop(void);\n");
    out.push_str("extern int32_t cobol_exception_code(void);\n");
    out.push_str("extern void cobol_exception_clear(void);\n");
    out.push_str(
        "extern int64_t cobol_invoke(void* obj, const char* method, int64_t* args, int32_t argc);\n",
    );
    out.push_str("/* COBOL 2014+ runtime declarations */\n");
    out.push_str("extern void cobol_validate(const char* target_name);\n");
    out.push_str(
        "extern uint32_t cobol_json_generate(const void* fields, uint32_t field_count, uint8_t* output, uint32_t output_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_json_parse(const uint8_t* json, uint32_t json_len, void* fields, uint32_t field_count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_xml_generate(const void* fields, uint32_t field_count, const uint8_t* root_name, uint32_t root_name_len, uint8_t* output, uint32_t output_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_xml_parse(const uint8_t* xml, uint32_t xml_len, void (*callback)(uint32_t, const uint8_t*, uint32_t, const uint8_t*, uint32_t));\n",
    );
    out.push_str("/* COBOL 2023+ runtime declarations */\n");
    out.push_str("extern uint32_t cobol_utf8_char_count(const uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str(
        "extern uint32_t cobol_utf8_substring(const uint8_t* src, uint32_t src_len, uint32_t start_char, uint32_t char_count, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern uint32_t cobol_utf8_upper(uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str("extern uint32_t cobol_utf8_lower(uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str("extern uint64_t cobol_thread_create(void (*func)(void*), void* arg);\n");
    out.push_str("extern uint32_t cobol_thread_join(uint64_t handle);\n");
    out.push_str("extern uint64_t cobol_mutex_create(void);\n");
    out.push_str("extern void cobol_mutex_lock(uint64_t handle);\n");
    out.push_str("extern void cobol_mutex_unlock(uint64_t handle);\n");
    out.push_str("extern void cobol_mutex_destroy(uint64_t handle);\n");
    out.push_str("/* Decimal arithmetic runtime declarations */\n");
    out.push_str(
        "extern void cobol_decimal_add(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n",
    );
    out.push_str(
        "extern void cobol_decimal_sub(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n",
    );
    out.push_str(
        "extern void cobol_decimal_mul(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n",
    );
    out.push_str(
        "extern void cobol_decimal_div(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n",
    );
    out.push_str(
        "extern int32_t cobol_decimal_cmp(const CobolDecimal* a, const CobolDecimal* b);\n",
    );
    out.push_str(
        "extern void cobol_decimal_from_int(int64_t value, int32_t scale, CobolDecimal* result);\n",
    );
    out.push_str("extern int64_t cobol_decimal_to_int64(const CobolDecimal* d);\n");
    out.push_str("extern double cobol_decimal_to_double(const CobolDecimal* d);\n");
    out.push_str("extern void cobol_decimal_from_double(double val, CobolDecimal* result);\n");
    out.push_str(
        "extern void cobol_decimal_from_string(const uint8_t* ptr, uint32_t len, CobolDecimal* result);\n",
    );
    out.push_str(
        "extern uint32_t cobol_decimal_to_display(const CobolDecimal* dec, uint8_t* buf, uint32_t buf_len, const uint8_t* pic_ptr, uint32_t pic_len);\n",
    );
    out.push_str("/* Screen section runtime declarations */\n");
    out.push_str("extern void cobol_screen_position(int32_t line, int32_t col);\n");
    out.push_str("extern void cobol_screen_clear(void);\n");
    out.push_str("extern void cobol_screen_clear_line(void);\n");
    out.push_str("extern void cobol_screen_highlight_on(void);\n");
    out.push_str("extern void cobol_screen_highlight_off(void);\n");
    out.push_str("extern void cobol_screen_reverse_on(void);\n");
    out.push_str("extern void cobol_screen_reverse_off(void);\n");
    out.push_str("extern void cobol_screen_reset_attrs(void);\n");
    out.push_str("/* NATIONAL (PIC N) runtime declarations */\n");
    out.push_str(
        "extern uint32_t cobol_func_national_of(const uint8_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_func_display_of(const uint16_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_move_to_national(const uint8_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern void cobol_display_national(const uint16_t* ptr, uint32_t len);\n");
    out.push_str(
        "extern void cobol_move_national_to_national(const uint16_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push('\n');
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_emit_c_declarations_exposes_named_runtime_abi_types() {
        let mut out = String::new();
        super::emit_c_declarations(&mut out);
        assert!(out.contains("typedef struct CobolDecimal"));
        assert!(out.contains("typedef struct CobolStringSource"));
        assert!(out.contains("typedef struct CobolUnstringTarget"));
        assert!(out.contains("typedef struct SortKey"));
        assert!(out.contains("const CobolStringSource* sources"));
        assert!(out.contains("CobolUnstringTarget* targets"));
        assert!(out.contains("const SortKey* keys"));
    }

    #[test]
    fn test_emit_c_declarations_keeps_runtime_boundary_hooks() {
        let mut out = String::new();
        super::emit_c_declarations(&mut out);
        assert!(out.contains("cobol_comm_enable"));
        assert!(out.contains("cobol_json_generate"));
        assert!(out.contains("cobol_json_parse"));
        assert!(out.contains("cobol_xml_generate"));
        assert!(out.contains("cobol_xml_parse"));
        assert!(out.contains("cobol_sort_buffer_sort"));
    }
}
