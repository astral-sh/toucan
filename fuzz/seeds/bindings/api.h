#define API_VERSION 3U
#define API_NAME "toucan"
typedef struct Item { int tag; double value; } Item;
typedef Item (*Callback)(Item, void *);
Item apply(Item input, Callback callback, void *context);
