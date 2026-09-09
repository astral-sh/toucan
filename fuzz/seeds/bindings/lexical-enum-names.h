struct Owner {
    enum Mode { FIRST = 1, SECOND = 2 } mode;
    union {
        struct { enum { NESTED = 3 } value; } inner;
        unsigned raw;
    } payload;
};
typedef struct { enum Tag { NAMED = 4 } tag; } Alias;
typedef enum { ANONYMOUS = 5 } Anonymous;
enum Forward;
struct Container { enum Forward { LATER = 6 } field; };
enum Mode consume(struct Owner value, enum Mode mode);
