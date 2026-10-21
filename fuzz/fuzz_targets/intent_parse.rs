//! The intent IR's parser on bytes from a model (#253): any text is read
//! or refused with a typed error, never a panic; what is read compiles.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(intent) = cad_intent::parse(text) {
        let compiled = cad_intent::compile(&intent);
        assert_eq!(compiled.len(), intent.operations.len());
    }
});
