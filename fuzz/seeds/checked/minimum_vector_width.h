__attribute__((min_vector_width((signed char)-1))) int f(int);
void h(void){__attribute__((min_vector_width(128))) int f(int);}
__attribute__((min_vector_width(64),min_vector_width(256))) int f(int x){return x;}
int query=sizeof(int __attribute__((min_vector_width())));
