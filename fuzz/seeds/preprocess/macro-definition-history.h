#define VALUE 1
#define ALIAS VALUE
#undef VALUE
#define VALUE 2
#undef ALIAS
#define FUNCTION(x, ...) x + __VA_ARGS__
#define EMPTY
#line 999 "logical.h"
#define VALUE_AFTER_LINE VALUE
VALUE_AFTER_LINE
