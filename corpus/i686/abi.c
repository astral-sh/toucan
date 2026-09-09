#include "abi.h"

struct I686Pair i686_pair(struct I686Pair value) {
    value.x += 2.0f;
    value.y -= 3.0f;
    return value;
}

struct I686Record i686_record(struct I686Record value, long delta) {
    value.tag = 7;
    value.value += delta;
    value.number *= 2.0;
    value.count ^= I686_MASK;
    return value;
}

struct I686Packed i686_packed(struct I686Packed value) {
    value.tag = 9;
    value.value += 19;
    return value;
}

struct I686Pair i686_callback(I686Callback callback, struct I686Pair value, void *context) {
    return callback(callback(value, 5, context), 11, context);
}

long i686_stack(long a, long b, long c, long d, long e, long f, long g, long h,
                double i, double j, double k, double l, double m, double n, double o, double p) {
    return a + 2*b + 3*c + 4*d + 5*e + 6*f + 7*g + 8*h
         + (long)(i + 2*j + 3*k + 4*l + 5*m + 6*n + 7*o + 8*p);
}

void i686_bits_write(struct I686Bits *value) {
    value->value = -9;
    value->flag = 6;
    value->rest = 381;
    value->tail = 12;
}

long i686_bits_read(const struct I686Bits *value) {
    return value->value + 100*value->flag + 1000*value->rest + 1000000*value->tail;
}
