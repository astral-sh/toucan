typedef unsigned __int64 Wide;
typedef signed _int8 Byte;
struct Values { Byte byte; __int16 word; __int32 number; Wide wide; };
Wide convert(struct Values value, Wide (*callback)(__int32)) {
    _int64 result = callback(value.number);
    return (Wide)result + value.wide;
}
