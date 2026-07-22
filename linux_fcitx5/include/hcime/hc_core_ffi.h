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
    uint8_t quick_consonants;
    uint8_t english_protection;
    uint8_t macro_in_english;
    uint8_t esc_restore_raw;
} HC_KeyRequest;

typedef struct HC_KeyResult {
    HC_State state;
    uint8_t handled;
} HC_KeyResult;

typedef struct HC_Utf8KeyResult {
    const char* composition_string;
    size_t length;
    int32_t status_flag;
    int32_t error_code;
    int32_t spell_check_status;
    uint8_t handled;
} HC_Utf8KeyResult;

enum HC_StatusFlag {
    HC_STATUS_IN_PROGRESS = 0,
    HC_STATUS_COMMIT = 1,
    HC_STATUS_ENGLISH_FALLBACK = 2,
    HC_STATUS_RECONVERSION_ACTIVE = 3,
    HC_STATUS_ESC_RESTORED_RAW = 4,
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

typedef struct HC_CandidateChar {
    uint8_t utf8[5];
    uint8_t byte_len;
} HC_CandidateChar;

typedef struct HC_HanNomResult {
    int32_t status_flag;
    int32_t error_code;
    char reading[256];
    uint16_t reading_len;
    const HC_CandidateChar* candidates;
    uint16_t candidate_count;
    uint16_t page;
    uint16_t total_candidates;
    uint8_t has_more;
    uint8_t handled;
} HC_HanNomResult;

/* V2 is additive: strings are borrowed until the next Hán Nôm call on this
 * session, allowing phrases and Extension-B+ characters without changing V1. */
typedef struct HC_HanNomCandidateText {
    const uint8_t* text;
    uint16_t text_len;
    const uint8_t* reading;
    uint16_t reading_len;
    uint8_t kind; /* exact=0, prediction=1, fallback=2, single=3 */
} HC_HanNomCandidateText;

typedef struct HC_HanNomResultV2 {
    int32_t status_flag;
    int32_t error_code;
    const uint8_t* reading;
    uint16_t reading_len;
    const HC_HanNomCandidateText* candidates;
    uint16_t candidate_count;
    uint8_t handled;
} HC_HanNomResultV2;

typedef struct HC_HanNomOptions {
    uint8_t phrase_prediction;
    uint8_t learning_enabled;
    const char* history_path;
} HC_HanNomOptions;

/* V3 leaves paging to Fcitx. Candidate pointers are borrowed until the next
 * Hán Nôm call on the same session or session destruction. */
typedef struct HC_HanNomOptionsV2 {
    uint8_t phrase_prediction;
    uint8_t learning_enabled;
    const char* history_path;
    const char* user_phrase_path;
} HC_HanNomOptionsV2;

typedef struct HC_HanNomResultV3 {
    int32_t status_flag;
    int32_t error_code;
    const uint8_t* reading;
    uint16_t reading_len;
    const HC_HanNomCandidateText* candidates;
    uint16_t candidate_count;
    uint16_t total_candidate_count;
    uint8_t page_size;
    uint8_t truncated;
    uint8_t handled;
} HC_HanNomResultV3;

enum HC_InputMode {
    HC_INPUT_TELEX = 0,
    HC_INPUT_VNI = 1,
    HC_INPUT_VIQR = 2,
    HC_INPUT_HAN_NOM_TELEX = 3,
    HC_INPUT_HAN_NOM_VNI = 4,
    HC_INPUT_HAN_NOM_VIQR = 5,
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
void hc_session_add_macro(void* session, const char* key, const char* value);
void hc_session_clear_macros(void* session);
HC_KeyResult hc_session_handle_key(void* session, const HC_KeyRequest* request);
HC_Utf8KeyResult hc_session_handle_key_utf8(void* session, const HC_KeyRequest* request);
int32_t hc_session_handle_key_hannom(void* session, const HC_KeyRequest* request, HC_HanNomResult* result);
int32_t hc_session_handle_key_hannom_v2(void* session, const HC_KeyRequest* request, HC_HanNomResultV2* result);
int32_t hc_session_select_hannom_candidate_v2(void* session, uint16_t index, HC_HanNomResultV2* result);
void hc_session_set_hannom_options(void* session, const HC_HanNomOptions* options);
int32_t hc_session_handle_key_hannom_v3(void* session, const HC_KeyRequest* request, HC_HanNomResultV3* result);
int32_t hc_session_select_hannom_candidate_v3(void* session, uint16_t absolute_index, HC_HanNomResultV3* result);
void hc_session_set_hannom_options_v2(void* session, const HC_HanNomOptionsV2* options);
void hc_session_reset_hannom_learning(void* session);
void hc_session_flush_hannom_learning(void* session);
int32_t hc_nom_dict_status(void* session);

#ifdef __cplusplus
}
#endif
