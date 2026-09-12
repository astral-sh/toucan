//! Target prototypes for libc calls with explicit destination-size arguments.

use crate::analyze::Analyzer;
use crate::integer::integer_to_type;
use crate::{Error, IntegerKind, Type, TypeKind};

pub(crate) struct FortifiedSignature {
    pub(crate) result: Type,
    pub(crate) parameters: Vec<Type>,
    pub(crate) variadic: bool,
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn fortified_signature(
        &self,
        name: &str,
        offset: usize,
    ) -> Result<Option<FortifiedSignature>, Error> {
        use Parameter::{Buffer, Bytes, ConstBuffer, ConstBytes, Int, Size, Stream, VaList};
        let Some((result, parameters, variadic)) = prototype(name) else {
            return Ok(None);
        };
        let convert = |parameter| -> Result<Type, Error> {
            let mut ty = match parameter {
                Bytes | ConstBytes => Type::new(TypeKind::Void),
                Buffer | ConstBuffer => Type::new(TypeKind::Integer(IntegerKind::Char)),
                Int => return Ok(Type::new(TypeKind::Integer(IntegerKind::Int))),
                Size => return Ok(integer_to_type(self.size_value(0))),
                VaList => {
                    let ty =
                        self.unit.typedefs.get("__builtin_va_list").ok_or_else(|| {
                            Error::new(offset, "target has no builtin va_list type")
                        })?;
                    return self.value_type(ty);
                }
                Stream => {
                    if self.unit.compiler == toucan_target::Compiler::Gnu {
                        return Ok(Type::new(TypeKind::Void).pointer());
                    }
                    let ty = self.unit.typedefs.get("FILE").ok_or_else(|| {
                        Error::new(
                            offset,
                            "fortified stream intrinsic requires a file-scope FILE typedef",
                        )
                    })?;
                    return Ok(ty.clone().pointer());
                }
            };
            ty.qualifiers.is_const = matches!(parameter, ConstBytes | ConstBuffer);
            Ok(ty.pointer())
        };
        Ok(Some(FortifiedSignature {
            result: convert(result)?,
            parameters: parameters
                .iter()
                .copied()
                .map(convert)
                .collect::<Result<_, _>>()?,
            variadic,
        }))
    }
}

#[derive(Clone, Copy)]
enum Parameter {
    Bytes,
    ConstBytes,
    Buffer,
    ConstBuffer,
    Int,
    Size,
    Stream,
    VaList,
}

pub(crate) fn is_fortified_builtin(name: &str) -> bool {
    prototype(name).is_some()
}
fn prototype(name: &str) -> Option<(Parameter, &'static [Parameter], bool)> {
    use Parameter::{Buffer, Bytes, ConstBuffer, ConstBytes, Int, Size, Stream, VaList};
    let signature: (Parameter, &[Parameter], bool) = match name {
        "__builtin___memcpy_chk" | "__builtin___memmove_chk" | "__builtin___mempcpy_chk" => {
            (Bytes, &[Bytes, ConstBytes, Size, Size], false)
        }
        "__builtin___memset_chk" => (Bytes, &[Bytes, Int, Size, Size], false),
        "__builtin___strcpy_chk" | "__builtin___stpcpy_chk" | "__builtin___strcat_chk" => {
            (Buffer, &[Buffer, ConstBuffer, Size], false)
        }
        "__builtin___strncpy_chk" | "__builtin___stpncpy_chk" | "__builtin___strncat_chk" => {
            (Buffer, &[Buffer, ConstBuffer, Size, Size], false)
        }
        "__builtin___sprintf_chk" => (Int, &[Buffer, Int, Size, ConstBuffer], true),
        "__builtin___snprintf_chk" => (Int, &[Buffer, Size, Int, Size, ConstBuffer], true),
        "__builtin___vsprintf_chk" => (Int, &[Buffer, Int, Size, ConstBuffer, VaList], false),
        "__builtin___vsnprintf_chk" => {
            (Int, &[Buffer, Size, Int, Size, ConstBuffer, VaList], false)
        }
        "__builtin___printf_chk" => (Int, &[Int, ConstBuffer], true),
        "__builtin___vprintf_chk" => (Int, &[Int, ConstBuffer, VaList], false),
        "__builtin___fprintf_chk" => (Int, &[Stream, Int, ConstBuffer], true),
        "__builtin___vfprintf_chk" => (Int, &[Stream, Int, ConstBuffer, VaList], false),
        _ => return None,
    };
    Some(signature)
}
