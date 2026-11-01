/* SPDX-License-Identifier: Apache-2.0
 *
 * opendoc.h: the OpenDoc Native SDK's C ABI (docs/176 sections 6 and 12.9).
 * Kept in step with crates/opendoc-ffi/src/lib.rs by the test
 * `the_header_declares_every_export` (crates/opendoc-ffi/tests/abi.rs).
 *
 * Ownership: every OdDocument* and OdBuffer* returned here belongs to the
 * caller, who frees it exactly once (opendoc_free, opendoc_buffer_free).
 * Inputs are borrowed for the call only. A function that fails returns NULL
 * and records a message on the calling thread (opendoc_last_error); the
 * message starts with a stable code such as "open:" or "template:".
 * Structured values are UTF-8 JSON in buffers; strings in are NUL-terminated
 * UTF-8.
 *
 * Handles are checked: a freed, double-freed or foreign handle or buffer is
 * refused (NULL plus an "invalid-argument" message, or a no-op for the frees
 * and the buffer accessors), never dereferenced. A pointer to fewer bytes than
 * the length given, or a string with no NUL, is still the caller's error.
 * Do not use one handle from two threads at once.
 */
#ifndef OPENDOC_H
#define OPENDOC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENDOC_ABI_VERSION 1

typedef struct OdDocument OdDocument;
typedef struct OdBuffer OdBuffer;

uint32_t opendoc_abi_version(void);
OdBuffer *opendoc_version(void);
OdBuffer *opendoc_last_error(void);

OdDocument *opendoc_open(const uint8_t *data, size_t len, const char *options_json);
OdDocument *opendoc_create(void);
OdDocument *opendoc_from_spec(const char *spec_json);
void opendoc_free(OdDocument *doc);

OdBuffer *opendoc_call(OdDocument *doc, const char *method, const char *arg_json);
OdBuffer *opendoc_save(OdDocument *doc, const char *format, const char *mode);
OdBuffer *opendoc_render_png(OdDocument *doc, uint32_t page, float dpi);
/* One page as SVG, UTF-8. */
OdBuffer *opendoc_render_svg(OdDocument *doc, uint32_t page);
OdBuffer *opendoc_to_pdf(OdDocument *doc);
OdBuffer *opendoc_compare(OdDocument *older, OdDocument *newer);
OdBuffer *opendoc_convert(const uint8_t *data, size_t len, const char *to);

const uint8_t *opendoc_buffer_data(const OdBuffer *buf);
size_t opendoc_buffer_len(const OdBuffer *buf);
void opendoc_buffer_free(OdBuffer *buf);

#ifdef __cplusplus
}
#endif

#endif /* OPENDOC_H */
