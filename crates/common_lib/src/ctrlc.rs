use crate::errors::ErrType;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

///
/// Добавить обработчик для отлавливания команды ctrl + с
///
/// returns: Result<Arc<AtomicBool>, ErrType>
///     Ok - переменная по которой прекращается работа метода
///
pub fn ctrlc_handler() -> Result<Arc<AtomicBool>, ErrType> {
    let stoper = Arc::new(AtomicBool::new(false));
    let stoper2 = stoper.clone();
    ctrlc::set_handler(move || {
        stoper2.store(true, Ordering::Release);
    })
    .map_err(|e| ErrType::CtrlcError(format!("Не удалось установить обработчик ctrlc. {:?}", e)))?;
    Ok(stoper)
}
