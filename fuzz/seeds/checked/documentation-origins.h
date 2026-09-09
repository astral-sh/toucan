/** Alias documentation. */
typedef struct Forward Alias;
/** Standalone forward. */
struct Forward;
/** Definition documentation. */
struct Forward {
    /** Field documentation. */
    int value;
    enum Nested { FIRST = 1, SECOND = 2 } choice;
};
typedef enum Choice Choice;
enum Choice { PICK = 3 };
struct Holder {
    enum { ANONYMOUS = 4 } item;
} __attribute__((aligned(8)));
