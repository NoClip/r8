/**
 * Chromium / Blink Compatible V8 C Embedding Interface.
 *
 * Safe Rust Reimplementation of Google V8 JavaScript Engine.
 */

#ifndef V8_H_
#define V8_H_

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Forward opaque declarations */
typedef struct V8Isolate V8Isolate;
typedef struct V8Context V8Context;
typedef struct V8Script V8Script;
typedef struct V8Value V8Value;

typedef V8Isolate v8_isolate_t;
typedef V8Context v8_context_t;
typedef V8Script v8_script_t;
typedef V8Value v8_value_t;

/* Isolate Management */
v8_isolate_t* v8_isolate_new(void);
void v8_isolate_dispose(v8_isolate_t* isolate);

/* Context Management */
v8_context_t* v8_context_new(v8_isolate_t* isolate);
void v8_context_dispose(v8_context_t* ctx);
v8_value_t* v8_context_global(v8_context_t* ctx);

/* Script Execution */
v8_script_t* v8_script_compile(v8_context_t* ctx, const char* source);
v8_value_t* v8_script_run(v8_script_t* script);
void v8_script_dispose(v8_script_t* script);

/* Value Construction */
v8_value_t* v8_value_new_number(v8_isolate_t* isolate, double value);
v8_value_t* v8_value_new_string(v8_isolate_t* isolate, const char* s);
v8_value_t* v8_value_new_boolean(v8_isolate_t* isolate, bool b);
v8_value_t* v8_value_new_undefined(v8_isolate_t* isolate);
v8_value_t* v8_value_new_null(v8_isolate_t* isolate);

/* Value Conversions & Type Queries */
double v8_value_to_number(const v8_value_t* val);
bool v8_value_to_boolean(const v8_value_t* val);
size_t v8_value_to_string(const v8_value_t* val, char* buf, size_t max_len);

bool v8_value_is_number(const v8_value_t* val);
bool v8_value_is_string(const v8_value_t* val);
bool v8_value_is_boolean(const v8_value_t* val);
bool v8_value_is_undefined(const v8_value_t* val);
bool v8_value_is_null(const v8_value_t* val);
bool v8_value_is_object(const v8_value_t* val);
bool v8_value_is_function(const v8_value_t* val);

void v8_value_dispose(v8_value_t* val);

/* Object Operations */
v8_value_t* v8_object_get(v8_value_t* obj, const char* prop);
bool v8_object_set(v8_value_t* obj, const char* prop, v8_value_t* val);

/* Function Invocation */
v8_value_t* v8_function_call(v8_value_t* func, v8_value_t* this_val, size_t argc, v8_value_t* const* argv);

/* Engine Metadata */
const char* v8_version(void);

#ifdef __cplusplus
}
#endif

#endif /* V8_H_ */
