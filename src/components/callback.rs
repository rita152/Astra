//! Shared native UI event callbacks. Protocol response handles remain in the agent layer.

use std::rc::Rc;

use gpui::{App, Window};

type UiEventHandler<E> = dyn Fn(E, &mut Window, &mut App) + 'static;

pub struct UiCallback<E>(Rc<UiEventHandler<E>>);

impl<E> Clone for UiCallback<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<E> UiCallback<E> {
    pub fn new(handler: impl Fn(E, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(handler))
    }

    pub(crate) fn emit(&self, event: E, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}
