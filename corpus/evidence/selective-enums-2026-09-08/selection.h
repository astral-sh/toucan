typedef enum { PC_COMPRESSED=2, PC_UNCOMPRESSED=4, PC_HYBRID=6 } point_conversion_form_t;
typedef point_conversion_form_t OtherAlias;
typedef enum Named { NAMED_ZERO=0, NAMED_ONE=1 } NamedAlias;
typedef enum { FIRST_ZERO=0, FIRST_ONE=1 } First, Second;
enum { ANON_ZERO=0, ANON_ONE=1 };
struct Holder { enum Nested { NESTED_ZERO=0, NESTED_ONE=1 } nested; };
enum Bits { BIT_ZERO=0, BIT_ONE=1 };
struct Flags { enum Bits value:1; };
