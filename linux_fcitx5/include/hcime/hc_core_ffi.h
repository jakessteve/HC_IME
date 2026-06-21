#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct HC_State {
    const uint16_t* composition_string;
    size_t length;
    int32_t status_flag;
    int32_t error_code;
    int32_t spell_check_status;
} HC_State;

typedef struct HC_ComposeRequest {
    const char* onset;
    const char* medial;
    const char* nucleus;
    const char* coda;
    int32_t tone;
    const char* trigger_case;
    const char* raw_input;
    uint8_t legacy_tone;
    int32_t boundary;
} HC_ComposeRequest;

typedef struct HC_RehydrateRequest {
    const char* committed_word;
    int32_t input_mode;
    int32_t trigger_kind;
    int32_t trigger_value;
} HC_RehydrateRequest;

typedef struct HC_KeyRequest {
    int32_t kind;
    const char* text;
    int32_t input_mode;
    uint8_t legacy_tone;
    uint8_t spell_check;
    uint8_t auto_restore;
} HC_KeyRequest;

typedef struct HC_KeyResult {
    HC_State state;
    uint8_t handled;
} HC_KeyResult;

enum HC_StatusFlag {
    HC_STATUS_IN_PROGRESS = 0,
    HC_STATUS_COMMIT = 1,
    HC_STATUS_ENGLISH_FALLBACK = 2,
    HC_STATUS_RECONVERSION_ACTIVE = 3,
};

enum HC_SpellCheckStatus {
    HC_SPELL_CHECK_VALID = 0,
    HC_SPELL_CHECK_INVALID = 1,
    HC_SPELL_CHECK_ENGLISH_FALLBACK = 2,
};

enum HC_ErrorCode {
    HC_ERROR_NONE = 0,
    HC_ERROR_NULL_POINTER = -1,
    HC_ERROR_INVALID_UTF8 = -2,
    HC_ERROR_INVALID_TONE = -3,
    HC_ERROR_INVALID_BOUNDARY = -4,
    HC_ERROR_INVALID_INPUT_MODE = -5,
    HC_ERROR_INVALID_EDIT_TRIGGER = -6,
    HC_ERROR_MISSING_REQUIRED_FIELD = -7,
    HC_ERROR_ENGINE_FAILURE = -8,
};

enum HC_KeyKind {
    HC_KEY_PRINTABLE = 0,
    HC_KEY_BACKSPACE = 1,
    HC_KEY_ENTER = 2,
    HC_KEY_SPACE = 3,
    HC_KEY_BOUNDARY = 4,
    HC_KEY_ESCAPE = 5,
    HC_KEY_OTHER = 6,
    HC_KEY_UNDO = 7,
};

enum HC_InputMode {
    HC_INPUT_TELEX = 0,
    HC_INPUT_VNI = 1,
    HC_INPUT_VIQR = 2,
};

HC_State hc_compose_with_request(const HC_ComposeRequest* request);
HC_State hc_rehydrate_with_request(const HC_RehydrateRequest* request);
HC_State hc_compose_from_parts(
    const char* onset,
    const char* medial,
    const char* nucleus,
    const char* coda,
    int32_t tone,
    const char* trigger_case,
    const char* raw_input,
    uint8_t legacy_tone,
    int32_t boundary
);
HC_State hc_rehydrate_apply(
    const char* committed_word,
    int32_t input_mode,
    int32_t trigger_kind,
    int32_t trigger_value
);
void hc_state_free(HC_State* state);

void* hc_session_new(int32_t input_mode, uint8_t legacy_tone);
void hc_session_free(void* session);
void hc_session_reset(void* session);
HC_KeyResult hc_session_handle_key(void* session, const HC_KeyRequest* request);

#ifdef __cplusplus
}
#endif
