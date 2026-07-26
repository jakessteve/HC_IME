#include "hcime_key_handler.h"

namespace hcime {

using namespace fcitx;

bool HcImeKeyHandler::HasCommandModifier(const Key& key) {
    const auto states = key.states();
    return states.test(KeyState::Ctrl) || states.test(KeyState::Alt) || states.test(KeyState::Super) ||
           states.test(KeyState::Super2) || states.test(KeyState::Hyper) || states.test(KeyState::Hyper2) ||
           states.test(KeyState::Meta);
}

bool HcImeKeyHandler::IsControlUtf8(const std::string& utf8) {
    if (utf8.size() != 1) return false;
    const auto ch = static_cast<unsigned char>(utf8.front());
    return ch < 0x20 || ch == 0x7F;
}

bool HcImeKeyHandler::IsPrintable(const Key& key, std::string& utf8) {
    if (HasCommandModifier(key) || key.isCursorMove() || key.isModifier()) return false;
    utf8 = Key::keySymToUTF8(key.sym());
    return !utf8.empty() && utf8.size() <= 4 && !IsControlUtf8(utf8);
}

bool HcImeKeyHandler::IsBoundaryChar(char ch) {
    switch (ch) {
        case ' ': case '.': case ',': case ';': case ':': case '!': case '?':
        case ')': case ']': case '}': case '/': case '\\': case '-': case '_':
        case '"': case '\'':
            return true;
        default: return false;
    }
}

bool HcImeKeyHandler::isBackspaceKey(const Key& key) {
    return key.check(FcitxKey_BackSpace) || key.check(FcitxKey_osfBackSpace);
}

bool HcImeKeyHandler::isDeleteKey(const Key& key) {
    return key.check(FcitxKey_Delete) || key.check(FcitxKey_KP_Delete) || key.check(FcitxKey_osfDelete) ||
           key.check(FcitxKey_DeleteChar) || key.check(FcitxKey_hpDeleteChar);
}

bool HcImeKeyHandler::isUndoKey(const Key& key) {
    return key.check(FcitxKey_Z, KeyState::Ctrl) || key.check(FcitxKey_z, KeyState::Ctrl);
}

bool HcImeKeyHandler::isEditingPassthroughKey(const Key& key) {
    return key.isCursorMove() || isDeleteKey(key) || key.check(FcitxKey_Home) || key.check(FcitxKey_KP_Home) ||
           key.check(FcitxKey_End) || key.check(FcitxKey_KP_End) || key.check(FcitxKey_Begin) ||
           key.check(FcitxKey_KP_Begin) || key.check(FcitxKey_Prior) || key.check(FcitxKey_KP_Prior) ||
           key.check(FcitxKey_Next) || key.check(FcitxKey_KP_Next) || key.check(FcitxKey_Insert) ||
           key.check(FcitxKey_KP_Insert) || key.check(FcitxKey_Tab) || key.check(FcitxKey_KP_Tab) ||
           key.check(FcitxKey_ISO_Left_Tab) || key.check(FcitxKey_KP_BackTab);
}

bool HcImeKeyHandler::isSpecialForwardingKey(const Key& key) {
    return isBackspaceKey(key) || key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter) ||
           key.check(FcitxKey_Escape);
}

int32_t HcImeKeyHandler::classify(const Key& key, const std::string& input) {
    if (isBackspaceKey(key)) return HC_KEY_BACKSPACE;
    if (key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter)) return HC_KEY_ENTER;
    if (key.check(FcitxKey_Escape)) return HC_KEY_ESCAPE;
    if (input == " ") return HC_KEY_SPACE;
    if (!input.empty() && IsBoundaryChar(input.front()) && input.size() == 1) return HC_KEY_BOUNDARY;
    if (!input.empty()) return HC_KEY_PRINTABLE;
    return HC_KEY_OTHER;
}

std::string HcImeKeyHandler::requestText(const Key& key) {
    std::string utf8;
    if (!IsPrintable(key, utf8)) return {};
    return utf8;
}

bool HcImeKeyHandler::isHanNomInputMode(int32_t mode) {
    return mode >= HC_INPUT_HAN_NOM_TELEX && mode <= HC_INPUT_HAN_NOM_VIQR;
}

HC_KeyRequest HcImeKeyHandler::makeKeyRequest(int32_t kind, const char* text, int32_t mode,
                                              uint8_t legacyTone, uint8_t spellCheck, uint8_t autoRestore,
                                              uint8_t quickConsonants, uint8_t englishProtection,
                                              uint8_t macroInEnglish, uint8_t escRestoreRaw) {
    return HC_KeyRequest{
        kind, text, mode,
        legacyTone, spellCheck, autoRestore, quickConsonants,
        englishProtection, macroInEnglish, escRestoreRaw,
    };
}

HC_KeyRequestV2 HcImeKeyHandler::makeKeyRequestV2(int32_t kind, const char* text, int32_t mode,
                                                  uint8_t legacyTone, uint8_t spellCheck, uint8_t autoRestore,
                                                  uint8_t quickConsonants, uint8_t englishProtection,
                                                  uint8_t macroInEnglish, uint8_t escRestoreRaw) {
    int32_t compositionMethod;
    uint8_t translationTarget;
    if (isHanNomInputMode(mode)) {
        compositionMethod = mode - HC_INPUT_HAN_NOM_TELEX;
        translationTarget = TRANSLATION_TARGET_HAN_NOM;
    } else {
        compositionMethod = mode;
        translationTarget = TRANSLATION_TARGET_VIETNAMESE;
    }
    return HC_KeyRequestV2{
        kind, text, compositionMethod, translationTarget,
        legacyTone, spellCheck, autoRestore, quickConsonants,
        englishProtection, macroInEnglish, escRestoreRaw,
    };
}

Utf8KeyResult HcImeKeyHandler::handleKeyUtf8(void* session, const HC_KeyRequest* request) {
    auto result = hc_session_handle_key_utf8(session, request);
    Utf8KeyResult output;
    output.statusFlag = result.status_flag;
    output.errorCode = result.error_code;
    output.spellCheckStatus = result.spell_check_status;
    output.handled = result.handled;
    if (result.length > 0 && result.composition_string != nullptr) {
        output.text.assign(result.composition_string, result.length);
    }
    return output;
}

SurroundingTextDelta computeSurroundingDiff(const std::string& oldText, const std::string& newText) {
    if (!utf8::validate(oldText) || !utf8::validate(newText)) {
        return {static_cast<unsigned int>(utf8::length(oldText)), newText};
    }

    size_t commonPrefixBytes = 0;
    auto oldIt = oldText.begin();
    auto newIt = newText.begin();
    while (oldIt != oldText.end() && newIt != newText.end()) {
        uint32_t oldChar = 0;
        uint32_t newChar = 0;
        auto oldNext = utf8::getNextChar(oldIt, oldText.end(), &oldChar);
        auto newNext = utf8::getNextChar(newIt, newText.end(), &newChar);
        if (oldChar != newChar) {
            break;
        }
        commonPrefixBytes += static_cast<size_t>(std::distance(oldIt, oldNext));
        oldIt = oldNext;
        newIt = newNext;
    }
    auto commonPrefixChars = static_cast<unsigned int>(utf8::length(oldText, 0, commonPrefixBytes));
    auto deleteChars = static_cast<unsigned int>(utf8::length(oldText) - commonPrefixChars);
    return {deleteChars, newText.substr(commonPrefixBytes)};
}

}  // namespace hcime
