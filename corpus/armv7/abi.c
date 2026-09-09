#include "abi.h"

struct Armv7Pair armv7_pair(struct Armv7Pair value) {
    value.x += 2.0f;
    value.y -= 3.0f;
    return value;
}

struct Armv7Record armv7_record(struct Armv7Record value, long delta) {
    value.tag = 7;
    value.value += delta;
    value.number *= 2.0;
    value.count ^= ARMV7_MASK;
    return value;
}

struct Armv7Pair armv7_callback(Armv7Callback callback, struct Armv7Pair value, void *context) {
    return callback(callback(value, 5.0f, context), 11.0f, context);
}

long armv7_stack(long a, long b, long c, long d, long e, long f, long g, long h,
                 double i, double j, double k, double l, double m, double n, double o, double p) {
    return a + 2*b + 3*c + 4*d + 5*e + 6*f + 7*g + 8*h
         + (long)(i + 2*j + 3*k + 4*l + 5*m + 6*n + 7*o + 8*p);
}

void armv7_packed_write(struct Armv7Packed *value, long delta) {
    value->tag = 9;
    value->value += delta;
}

void armv7_bits_write(struct Armv7Bits *value) {
    value->value = -9;
    value->flag = 6;
    value->rest = 381;
    value->tail = 12;
}

long armv7_bits_read(const struct Armv7Bits *value) {
    return value->value + 100*value->flag + 1000*value->rest + 1000000*value->tail;
}
