//! The sealed [`Derived<T>`] wrapper — the load-bearing type behind
//! non-negotiable #2 ("health is always derived, never hand-set").

use serde::Serialize;

/// A value produced by the derivation chain in [`crate::derive`] — and
/// constructible *nowhere else*.
///
/// The inner field is private and the only constructor, [`Derived::seal`], is
/// `pub(in crate::derive)`, so only code inside the derive module can mint one.
/// Every signal-cache field in the domain model is typed
/// `Option<Derived<Signal>>`, which turns "health is always derived" (plan
/// `§2.5`) into a **compile-time** guarantee: you cannot drop a hand-written
/// `Signal::Green` into a cache field without routing it through
/// [`evaluate_metric`](crate::derive::evaluate_metric) or
/// [`reduce_worst_of`](crate::derive::reduce_worst_of).
///
/// Deliberately **`Serialize` but not `Deserialize`**: exporting a derived value
/// (to a UI DTO) is fine, but deserializing one would be a fabrication backdoor —
/// and the store never treats a cached signal as authoritative; on a cache miss
/// it recomputes (plan `§2.5`: "绝不把缓存当权威"). Rehydration therefore goes
/// through the chain, not through `serde`.
///
/// The seal is enforced by the compiler. Both of these fail to compile:
///
/// ```compile_fail
/// use bw_core::derive::Derived;
/// use bw_core::Signal;
/// // `seal` is pub(in crate::derive) — invisible outside the kernel's derive mod.
/// let _ = Derived::seal(Signal::Green);
/// ```
///
/// ```compile_fail
/// use bw_core::derive::Derived;
/// use bw_core::Signal;
/// // The inner field is private — no struct-literal fabrication either.
/// let _ = Derived(Signal::Green);
/// ```
///
/// Reading a derived value back out is, of course, allowed:
///
/// ```
/// use bw_core::derive::reduce_worst_of;
/// use bw_core::Signal;
/// let d = reduce_worst_of([Signal::Green, Signal::Amber]);
/// assert_eq!(d.into_inner(), Signal::Amber);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(transparent)]
pub struct Derived<T>(T);

impl<T> Derived<T> {
    /// The single internal mint. `pub(in crate::derive)` ⇒ callable only from
    /// within the derivation module (in practice: [`evaluate_metric`] and
    /// [`reduce_worst_of`]).
    ///
    /// [`evaluate_metric`]: crate::derive::evaluate_metric
    /// [`reduce_worst_of`]: crate::derive::reduce_worst_of
    pub(in crate::derive) fn seal(value: T) -> Self {
        Derived(value)
    }

    /// Borrow the derived value.
    pub fn get(&self) -> &T {
        &self.0
    }
}

impl<T: Copy> Derived<T> {
    /// Copy the derived value out (for `Copy` payloads such as [`crate::Signal`]).
    pub fn into_inner(self) -> T {
        self.0
    }
}
