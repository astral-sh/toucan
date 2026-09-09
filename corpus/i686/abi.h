#define I686_MAGIC 0x7351a20u
#define I686_MASK 0x1122334455667788ULL

struct I686Pair { float x, y; };
struct I686Record {
    char tag;
    long value;
    double number;
    void *context;
    long long count;
};
struct __attribute__((packed)) I686Packed { char tag; long value; };
struct I686Bits { signed int value:5; unsigned int flag:3; unsigned int rest:9; char tail; };

typedef struct I686Pair (*I686Callback)(struct I686Pair, long, void *);

struct I686Pair i686_pair(struct I686Pair value);
struct I686Record i686_record(struct I686Record value, long delta);
struct I686Packed i686_packed(struct I686Packed value);
struct I686Pair i686_callback(I686Callback callback, struct I686Pair value, void *context);
long i686_stack(long a, long b, long c, long d, long e, long f, long g, long h,
                double i, double j, double k, double l, double m, double n, double o, double p);
void i686_bits_write(struct I686Bits *value);
long i686_bits_read(const struct I686Bits *value);
