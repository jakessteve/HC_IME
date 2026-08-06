// Bridging header for the HC_IME macOS InputMethodKit frontend.
//
// The C ABI is deliberately shared verbatim with the Fcitx5 addon so both
// frontends consume the same contract. Do not fork this header — if the Rust
// ABI changes, `linux_fcitx5/include/hcime/hc_core_ffi.h` is the source of
// truth and this include follows it.

#import "../../linux_fcitx5/include/hcime/hc_core_ffi.h"
