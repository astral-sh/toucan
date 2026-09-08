#include <stdatomic.h>
#include "api.h"
AtomicInt GLOBAL=1;
const AtomicInt CONSTANT=17;
volatile AtomicInt DEVICE=19;
void c_increment(unsigned n){for(unsigned i=0;i<n;i++)atomic_fetch_add_explicit(&GLOBAL,1,memory_order_seq_cst);}
void c_fields(struct Fields*p,int*data){atomic_fetch_add(&p->a,3);atomic_store(&p->b,1);atomic_store(&p->p,data);}
void c_pair_init(AtomicPair*p){struct Pair value={1.25f,2.5f};atomic_init(p,value);}
float c_pair_sum(const AtomicPair*p){struct Pair value=atomic_load(p);return value.a+value.b;}
void c_pair_write(AtomicPair*p){struct Pair value={4.0f,8.0f};atomic_store(p,value);}
extern int rust_storage(struct Fields*,AtomicPair*);
int c_callback_storage(struct Fields*p,AtomicPair*q){return rust_storage(p,q);}
void c_union_increment(union U*p){atomic_fetch_add(&p->a,2);}
AtomicChar c_i8(AtomicChar x){return x-1;}
AtomicShort c_i16(AtomicShort x){return x-1;}
AtomicInt c_i32(AtomicInt x){return x-1;}
AtomicLong c_i64(AtomicLong x){return x-1;}
AtomicBool c_b(AtomicBool x){return !x;}
AtomicFloat c_f(AtomicFloat x){return x+1.5f;}
AtomicDouble c_d(AtomicDouble x){return x+1.5;}
AtomicPointer c_p(AtomicPointer x){return x+1;}
AtomicEnum c_enum(AtomicEnum x){return x;}
AtomicFunction c_function(AtomicFunction x){return x;}
AtomicInt c_callback(Callback f,AtomicInt x){return f(x);}
AtomicInt c_read_constant(void){return CONSTANT;}
AtomicInt c_read_device(void){return DEVICE;}
int c_many(AtomicChar a,AtomicChar b,AtomicChar c,AtomicChar d,AtomicChar e,AtomicChar f,AtomicChar g,AtomicChar h,AtomicChar i,AtomicChar j,AtomicChar k,AtomicChar l){return a+b+c+d+e+f+g+h+i+j+k+l;}
AtomicChar c_callback_i8(AtomicChar(*f)(AtomicChar),AtomicChar x){return f(x);}
AtomicShort c_callback_i16(AtomicShort(*f)(AtomicShort),AtomicShort x){return f(x);}
AtomicLong c_callback_i64(AtomicLong(*f)(AtomicLong),AtomicLong x){return f(x);}
AtomicBool c_callback_b(AtomicBool(*f)(AtomicBool),AtomicBool x){return f(x);}
AtomicFloat c_callback_f(AtomicFloat(*f)(AtomicFloat),AtomicFloat x){return f(x);}
AtomicDouble c_callback_d(AtomicDouble(*f)(AtomicDouble),AtomicDouble x){return f(x);}
AtomicPointer c_callback_p(AtomicPointer(*f)(AtomicPointer),AtomicPointer x){return f(x);}
