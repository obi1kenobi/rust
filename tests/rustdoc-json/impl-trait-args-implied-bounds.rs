pub trait SizedOnly: Sized {}
impl<T: Sized> SizedOnly for T {}

pub trait StaticOnly: 'static {}
impl<T: 'static> StaticOnly for T {}

//@ is "$.index[?(@.name=='takes_unsized')].inner.function.sig.inputs[0][0]" '"arg"'
//@ has "$.index[?(@.name=='takes_unsized')].inner.function.sig.inputs[0][1].impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ !has "$.index[?(@.name=='takes_unsized')].inner.function.sig.inputs[0][1].impl_trait.implied_bounds[?(@.trait_bound.trait.path=='SizedOnly')]"
pub fn takes_unsized(arg: impl SizedOnly + ?Sized) {}

//@ is "$.index[?(@.name=='takes_static')].inner.function.sig.inputs[0][0]" '"arg"'
//@ has "$.index[?(@.name=='takes_static')].inner.function.sig.inputs[0][1].impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='takes_static')].inner.function.sig.inputs[0][1].impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub fn takes_static(arg: impl StaticOnly) {}
