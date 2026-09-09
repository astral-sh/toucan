typedef unsigned long size_t;
struct Item { const char *name; size_t count; struct Item *next; };
enum Flags { A = 1, B = 1 << 2, C = A | B };
typedef int (*Callback)(const struct Item *, void *);
int visit(struct Item *, Callback callback, void *context);
