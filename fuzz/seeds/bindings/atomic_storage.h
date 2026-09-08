typedef _Atomic(int) AtomicInt;
struct Counter{AtomicInt value;};
struct Pair{float x,y;};
typedef _Atomic(struct Pair) AtomicPair;
AtomicInt roundtrip(AtomicInt);
void access(struct Counter*, AtomicPair*);
void callback(AtomicInt(*)(AtomicInt));
