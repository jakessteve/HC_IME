use crate::composition::CompositionEngine;
use crate::translation::{HanNomTranslator, Translator};
use crate::types::InputMode;

pub struct Session {
    pub composition: CompositionEngine,
    pub translator: Option<Box<dyn Translator>>,
}

impl Session {
    pub fn new(mode: InputMode, legacy_tone: bool) -> Self {
        let translator: Option<Box<dyn Translator>> = if matches!(
            mode,
            InputMode::HanNomTelex | InputMode::HanNomVni | InputMode::HanNomViqr
        ) {
            Some(Box::new(HanNomTranslator::new()))
        } else {
            None
        };
        Self {
            composition: CompositionEngine::new(mode, legacy_tone),
            translator,
        }
    }

    pub fn reset(&mut self) {
        self.composition.reset();
    }

    pub fn translator_mut(&mut self) -> Option<&mut HanNomTranslator> {
        self.translator
            .as_mut()
            .and_then(|t| t.as_any_mut().downcast_mut::<HanNomTranslator>())
    }
}
