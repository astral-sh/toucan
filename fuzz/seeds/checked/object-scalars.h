static const int literal = 7;
static const int expression = 3 + 4;
static const int read_object = literal + 5;
static const int selected = __builtin_choose_expr(1, literal, 0);
static const unsigned __int128 wide = ((unsigned __int128)1 << 96) | 17;
static const __int128 negative = -((__int128)1 << 100);
extern const int declared;
const int declared = 9;
