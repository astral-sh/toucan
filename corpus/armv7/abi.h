#define ARMV7_MAGIC 0x7351a20u
#define ARMV7_MASK 0x1122334455667788ULL

struct Armv7Pair { float x, y; };
struct Armv7Record {
    char tag;
    long value;
    double number;
    void *context;
    long long count;
};
struct __attribute__((packed)) Armv7Packed { char tag; long value; };
struct Armv7Bits {
    signed int value:5;
    unsigned int flag:3;
    unsigned int rest:9;
    char tail;
};

typedef struct Armv7Pair (*Armv7Callback)(struct Armv7Pair, float, void *);

struct Armv7Pair armv7_pair(struct Armv7Pair value);
struct Armv7Record armv7_record(struct Armv7Record value, long delta);
struct Armv7Pair armv7_callback(Armv7Callback callback, struct Armv7Pair value, void *context);
long armv7_stack(long a, long b, long c, long d, long e, long f, long g, long h,
                 double i, double j, double k, double l, double m, double n, double o, double p);
void armv7_packed_write(struct Armv7Packed *value, long delta);
void armv7_bits_write(struct Armv7Bits *value);
long armv7_bits_read(const struct Armv7Bits *value);
