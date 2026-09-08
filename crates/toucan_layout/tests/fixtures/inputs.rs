use engine::layout::{Annotation, Array, BuiltinType, Record, RecordField, RecordKind, Type, TypeVariant};

pub fn inputs() -> Vec<Type<()>> {
    let builtin = |kind| Type { layout: (), annotations: vec![], variant: TypeVariant::Builtin(kind) };
    let mut inputs = vec![];
    for kind in [BuiltinType::Char, BuiltinType::UnsignedChar, BuiltinType::Short, BuiltinType::Int, BuiltinType::Long, BuiltinType::LongLong, BuiltinType::I128, BuiltinType::Float, BuiltinType::Double, BuiltinType::Pointer] {
        inputs.push(builtin(kind));
    }
    for values in [vec![],vec![-1,1],vec![0,255],vec![-1,256],vec![1i128<<63]] {
        for annotations in [vec![],vec![Annotation::AttrPacked],vec![Annotation::Align(Some(128))]] {
            inputs.push(Type { layout: (), annotations, variant: TypeVariant::Enum(values.clone()) });
        }
    }
    for kind in [RecordKind::Struct, RecordKind::Union] {
        for annotations in [vec![],vec![Annotation::AttrPacked],vec![Annotation::PragmaPack(16)],vec![Annotation::Align(Some(128))]] {
            inputs.push(Type { layout: (),annotations,variant: TypeVariant::Record(Record {kind,fields:vec![
                RecordField {layout:None,annotations:vec![],named:true,bit_width:Some(3),ty:builtin(BuiltinType::Char)},
                RecordField {layout:None,annotations:vec![Annotation::Align(Some(32))],named:true,bit_width:Some(9),ty:builtin(BuiltinType::Int)},
                RecordField {layout:None,annotations:vec![],named:false,bit_width:Some(0),ty:builtin(BuiltinType::LongLong)},
                RecordField {layout:None,annotations:vec![],named:true,bit_width:None,ty:builtin(BuiltinType::Char)},
            ]})});
        }
    }
    let nested = inputs.last().unwrap().clone();
    inputs.push(Type {layout:(),annotations:vec![Annotation::Align(Some(256))],variant:TypeVariant::Typedef(Box::new(nested))});
    inputs.push(Type {layout:(),annotations:vec![],variant:TypeVariant::Array(Array {element_type:Box::new(inputs.last().unwrap().clone()),num_elements:Some(3)})});
    inputs
}
