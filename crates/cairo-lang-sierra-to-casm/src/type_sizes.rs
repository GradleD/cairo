use std::collections::HashMap;

use cairo_lang_sierra::extensions::core::{CoreLibfunc, CoreType, CoreTypeConcrete};
use cairo_lang_sierra::extensions::starknet::StarkNetTypeConcrete;
use cairo_lang_sierra::ids::ConcreteTypeId;
use cairo_lang_sierra::program::Program;
use cairo_lang_sierra::program_registry::ProgramRegistry;

pub type TypeSizeMap = HashMap<ConcreteTypeId, i16>;

/// Returns a mapping for the sizes of all types for the given program.
pub fn get_type_size_map(
    program: &Program,
    registry: &ProgramRegistry<CoreType, CoreLibfunc>,
) -> Option<TypeSizeMap> {
    let mut type_sizes = TypeSizeMap::new();
    for declaration in &program.type_declarations {
        let ty = registry.get_type(&declaration.id).ok()?;
        let size = match ty {
            CoreTypeConcrete::Felt252(_)
            | CoreTypeConcrete::GasBuiltin(_)
            | CoreTypeConcrete::Bitwise(_)
            | CoreTypeConcrete::BuiltinCosts(_)
            | CoreTypeConcrete::EcOp(_)
            | CoreTypeConcrete::Nullable(_)
            | CoreTypeConcrete::Uint8(_)
            | CoreTypeConcrete::Uint16(_)
            | CoreTypeConcrete::Uint32(_)
            | CoreTypeConcrete::Uint64(_)
            | CoreTypeConcrete::Uint128(_)
            | CoreTypeConcrete::RangeCheck(_)
            | CoreTypeConcrete::Box(_)
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::System(_))
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::StorageBaseAddress(_))
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::StorageAddress(_))
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::ContractAddress(_))
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::ClassHash(_))
            | CoreTypeConcrete::StarkNet(StarkNetTypeConcrete::Secp256Point(_))
            | CoreTypeConcrete::Pedersen(_)
            | CoreTypeConcrete::Poseidon(_)
            | CoreTypeConcrete::Felt252Dict(_)
            | CoreTypeConcrete::Felt252DictEntry(_)
            | CoreTypeConcrete::SegmentArena(_) => Some(1),
            CoreTypeConcrete::Array(_)
            | CoreTypeConcrete::Span(_)
            | CoreTypeConcrete::EcPoint(_)
            | CoreTypeConcrete::SquashedFelt252Dict(_) => Some(2),
            CoreTypeConcrete::NonZero(wrapped_ty)
            | CoreTypeConcrete::Snapshot(wrapped_ty)
            | CoreTypeConcrete::Uninitialized(wrapped_ty) => {
                type_sizes.get(&wrapped_ty.ty).cloned()
            }
            CoreTypeConcrete::EcState(_) => Some(3),
            CoreTypeConcrete::Uint128MulGuarantee(_) => Some(4),
            CoreTypeConcrete::Enum(enum_type) => Some(
                1 + enum_type
                    .variants
                    .iter()
                    .map(|variant| type_sizes[variant])
                    .max()
                    .unwrap_or_default(),
            ),
            CoreTypeConcrete::Struct(struct_type) => {
                Some(struct_type.members.iter().map(|member| type_sizes[member]).sum())
            }
        }?;
        type_sizes.insert(declaration.id.clone(), size);
    }
    Some(type_sizes)
}