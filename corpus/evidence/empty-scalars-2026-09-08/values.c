
_Bool boolean = {};
int integer = {};
const unsigned long qualified = {{{}}};
enum E { Nonzero = 7 }; enum E enumeration = {};
float single = {}; double real = {}; long double extended = {};
double _Complex complex = {};
int *pointer = {}; int (*function_pointer)(void) = {};
struct S { int x; int *p; }; struct S record = {.x = {}, .p = {}};
int elements[2] = {{}, {}};
int literal = (int){}; double nested = (double){{{}}};
int *null_literal = (int*){};
int local(void) { int zero = {}; return zero + (int){}; }

int main(void) {
    unsigned long long bits = 1;
    __builtin_memcpy(&bits, &real, sizeof(real));
    if (bits || boolean || integer || qualified || enumeration || single || extended) return 1;
    if (__builtin_signbit(single) || __builtin_signbit(real) || __builtin_signbit(extended)) return 2;
    if (__real__ complex || __imag__ complex || __builtin_signbit(__real__ complex) || __builtin_signbit(__imag__ complex)) return 3;
    if (pointer || function_pointer || record.x || record.p || elements[0] || elements[1]) return 4;
    if (literal || nested || null_literal || local()) return 5;
    return 0;
}
