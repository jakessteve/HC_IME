#pragma once

#include "hcime/hc_core_ffi.h"

#include <fcitx/inputcontext.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/utf8.h>

#include <string>
#include <vector>

namespace hcime {

struct SessionHandle {
    void* ptr = nullptr;
    ~SessionHandle() {
        if (ptr != nullptr) {
            hc_session_free(ptr);
        }
    }
};

struct IcuuidHash {
    size_t operator()(const fcitx::ICUUID& uuid) const noexcept {
        size_t value = 0;
        for (auto byte : uuid) {
            value = value * 131u + byte;
        }
        return value;
    }
};

enum class PerAppMode {
    Global,
    ForceEnglish,
    ForceVietnamese,
};

enum class SmartSwitchState {
    Unknown,
    Vietnamese,
    English,
};

struct ContextState {
    SessionHandle session;
    bool hasActivePreedit = false;
    bool hanNomCandidatePhase = false;
    unsigned int lastCommitTrailingChars = 0;
    bool surroundingTextEnabled = false;
    PerAppMode perAppMode = PerAppMode::Global;
    SmartSwitchState smartSwitchState = SmartSwitchState::Unknown;
    std::string previousSurroundingText;
};

struct SurroundingTextDelta {
    unsigned int deleteChars = 0;
    std::string insertText;
};

struct Utf8KeyResult {
    std::string text;
    int32_t statusFlag = HC_STATUS_IN_PROGRESS;
    int32_t errorCode = HC_ERROR_NONE;
    int32_t spellCheckStatus = HC_SPELL_CHECK_VALID;
    uint8_t handled = 0;
};

SurroundingTextDelta computeSurroundingDiff(const std::string& oldText, const std::string& newText);

class HcImeKeyHandler {
public:
    static bool HasCommandModifier(const fcitx::Key& key);
    static bool IsControlUtf8(const std::string& utf8);
    static bool IsPrintable(const fcitx::Key& key, std::string& utf8);
    static bool IsBoundaryChar(char ch);

    static bool isBackspaceKey(const fcitx::Key& key);
    static bool isDeleteKey(const fcitx::Key& key);
    static bool isUndoKey(const fcitx::Key& key);
    static bool isEditingPassthroughKey(const fcitx::Key& key);
    static bool isSpecialForwardingKey(const fcitx::Key& key);

    static int32_t classify(const fcitx::Key& key, const std::string& input);
    static std::string requestText(const fcitx::Key& key);

    static bool isHanNomInputMode(int32_t mode);

    static HC_KeyRequest makeKeyRequest(int32_t kind, const char* text, int32_t mode,
                                        uint8_t legacyTone, uint8_t spellCheck, uint8_t autoRestore,
                                        uint8_t quickConsonants, uint8_t englishProtection,
                                        uint8_t macroInEnglish, uint8_t escRestoreRaw);

    static HC_KeyRequestV2 makeKeyRequestV2(int32_t kind, const char* text, int32_t mode,
                                            uint8_t legacyTone, uint8_t spellCheck, uint8_t autoRestore,
                                            uint8_t quickConsonants, uint8_t englishProtection,
                                            uint8_t macroInEnglish, uint8_t escRestoreRaw);

    static Utf8KeyResult handleKeyUtf8(void* session, const HC_KeyRequest* request);
};

}  // namespace hcime
