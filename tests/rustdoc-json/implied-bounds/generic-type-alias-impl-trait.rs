#![feature(type_alias_impl_trait)]

use std::fmt::Debug;

pub trait NeedsSized: Sized {}
impl<T: Sized> NeedsSized for T {}

pub trait StaticOnly: 'static {}
impl<T: 'static> StaticOnly for T {}

// The "FIXME" comments below are for assertions that should be enabled, but currently can't be.
// Attempts to include implied outlives bounds on the inner opaque for the `&'a` ref variants
// currently ICE rustdoc; the borrow requires the inner opaque to outlive `'a`.

//@ has "$.index[?(@.name=='Opaque')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
//@ has "$.index[?(@.name=='Opaque')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='Opaque')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ has "$.index[?(@.name=='Opaque')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
pub type Opaque<'a, T> = impl Debug + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='OpaqueRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
//@ has "$.index[?(@.name=='OpaqueRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Debug')]"
// FIXME: @ has "$.index[?(@.name=='OpaqueRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
pub type OpaqueRef<'a, T> = &'a (impl Debug + PartialEq<T>);

//@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
//@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ count "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.type_alias.type.impl_trait.implied_bounds[*]" 0
pub type OpaqueMaybeUnsized<'a, T> = impl Debug + PartialEq<T> + ?Sized + 'a;

//@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
//@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ !has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Debug')]"
//@ !has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
// FIXME: @ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
pub type OpaqueMaybeUnsizedRef<'a, T> = &'a (impl Debug + PartialEq<T> + ?Sized);

//@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
pub type OpaqueSizedViaTrait<'a, T> = impl NeedsSized + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
// FIXME: @ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
pub type OpaqueSizedViaTraitRef<'a, T> = &'a (impl NeedsSized + PartialEq<T>);

//@ has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueOverridden')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
pub type OpaqueOverridden<'a, T> = impl NeedsSized + PartialEq<T> + ?Sized + 'a;

//@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
// FIXME: @ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
pub type OpaqueOverriddenRef<'a, T> = &'a (impl NeedsSized + PartialEq<T> + ?Sized);

//@ has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='OpaqueStatic')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub type OpaqueStatic<'a, T> = impl StaticOnly + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='OpaqueStaticRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub type OpaqueStaticRef<'a, T> = &'a (impl StaticOnly + PartialEq<T>);

//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.type_alias.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub type OpaqueStaticMaybeUnsized<'a, T> = impl StaticOnly + PartialEq<T> + ?Sized + 'a;

//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.type_alias.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub type OpaqueStaticMaybeUnsizedRef<'a, T> = &'a (impl StaticOnly + PartialEq<T> + ?Sized);
