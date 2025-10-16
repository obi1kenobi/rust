pub trait StaticOnly: 'static {}
impl<T: 'static> StaticOnly for T {}

//@ has "$.index[?(@.name=='returns_static')].inner.function.sig.output.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ has "$.index[?(@.name=='returns_static')].inner.function.sig.output.impl_trait.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='returns_static')].inner.function.sig.output.impl_trait.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub fn returns_static() -> impl StaticOnly {
    0u8
}
