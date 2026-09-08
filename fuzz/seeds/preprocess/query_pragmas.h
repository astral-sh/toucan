#define ATTRIBUTE(x) __has_attribute(x)
int ordinary = __has_attribute(_Pragma("once") aligned);
int wrapped = ATTRIBUTE(_Pragma("once") aligned);
#if defined(__has_feature)
int raw = __has_feature(_Pragma("once") c_atomic);
#endif
#if __has_attribute(_Pragma("once") aligned)
int conditional;
#endif
