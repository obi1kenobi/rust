#![feature(impl_trait_in_assoc_type)]

use std::fmt::Debug;

pub trait NeedsSized: Sized {}
impl<T: Sized> NeedsSized for T {}

pub trait StaticOnly: 'static {}
impl<T: 'static> StaticOnly for T {}

pub trait AssocTypes<'a, T> {
    type Opaque;
    type OpaqueRef;
    type OpaqueMaybeUnsized: ?Sized;
    type OpaqueMaybeUnsizedRef;
    type OpaqueSizedViaTrait;
    type OpaqueSizedViaTraitRef;
    type OpaqueOverridden;
    type OpaqueOverriddenRef;
    type OpaqueStatic;
    type OpaqueStaticRef;
    type OpaqueStaticMaybeUnsized: ?Sized;
    type OpaqueStaticMaybeUnsizedRef;
}

pub struct Holder;

impl<'a, T: 'a> AssocTypes<'a, T> for Holder {
    //@ has "$.index[?(@.name=='Opaque')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ has "$.index[?(@.name=='Opaque')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='Opaque')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ has "$.index[?(@.name=='Opaque')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    type Opaque = impl Debug + PartialEq<T> + 'a;

    //@ has "$.index[?(@.name=='OpaqueRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ has "$.index[?(@.name=='OpaqueRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ has "$.index[?(@.name=='OpaqueRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
    type OpaqueRef = &'a (impl Debug + PartialEq<T>);

    //@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ !has "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
    //@ count "$.index[?(@.name=='OpaqueMaybeUnsized')].inner.assoc_type.type.impl_trait.implied_bounds[*]" 0
    type OpaqueMaybeUnsized = impl Debug + PartialEq<T> + ?Sized + 'a;

    //@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ !has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Debug')]"
    //@ !has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
    //@ has "$.index[?(@.name=='OpaqueMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
    type OpaqueMaybeUnsizedRef = &'a (impl Debug + PartialEq<T> + ?Sized);

    //@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueSizedViaTrait')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    type OpaqueSizedViaTrait = impl NeedsSized + PartialEq<T> + 'a;

    //@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueSizedViaTraitRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
    type OpaqueSizedViaTraitRef = &'a (impl NeedsSized + PartialEq<T>);

    //@ has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueOverridden')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    type OpaqueOverridden = impl NeedsSized + PartialEq<T> + ?Sized + 'a;

    //@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
    //@ has "$.index[?(@.name=='OpaqueOverriddenRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'a\")]"
    type OpaqueOverriddenRef = &'a (impl NeedsSized + PartialEq<T> + ?Sized);

    //@ has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    //@ has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
    //@ has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ !has "$.index[?(@.name=='OpaqueStatic')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    type OpaqueStatic = impl StaticOnly + PartialEq<T> + 'a;

    //@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    //@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
    //@ has "$.index[?(@.name=='OpaqueStaticRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
    //@ !has "$.index[?(@.name=='OpaqueStaticRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    type OpaqueStaticRef = &'a (impl StaticOnly + PartialEq<T>);

    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.bounds[?(@.outlives==\"'a\")]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
    //@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
    //@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsized')].inner.assoc_type.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    type OpaqueStaticMaybeUnsized = impl StaticOnly + PartialEq<T> + ?Sized + 'a;

    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='PartialEq' && @.trait_bound.trait.args.angle_bracketed.args[0].type.generic=='T')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
    //@ has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
    //@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
    //@ !has "$.index[?(@.name=='OpaqueStaticMaybeUnsizedRef')].inner.assoc_type.type.borrowed_ref.type.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
    type OpaqueStaticMaybeUnsizedRef = &'a (impl StaticOnly + PartialEq<T> + ?Sized);
}
