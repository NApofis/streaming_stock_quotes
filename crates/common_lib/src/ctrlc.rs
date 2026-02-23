use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::errors::ErrType;

pub fn ctrlc_handler() -> Result<Arc<AtomicBool>, ErrType> {
    let stoper = Arc::new(AtomicBool::new(false));
    let stoper2 = stoper.clone();
    let ctrl_handler = ctrlc::set_handler(move || {
        stoper2.store(true, Ordering::Release);
    });

    if ctrl_handler.is_err() {
        Err(ErrType::CtrlcError(format!("Не удалось установить обработчик ctrlc. {:?}", ctrl_handler)))?
    };
    Ok(stoper)
}