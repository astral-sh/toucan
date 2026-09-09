#define CAT(a,b) a##b
#define ATTRIBUTE(x) __has_attribute(x)
#define WRAP(x) __has_feature(x)
#define FEATURE c_atomic
#if defined(__has_feature)
int feature = __has_feature(c_atomic);
int extension = __has_extension(__c_atomic__);
int wrapped = WRAP(FEATURE);
int module = __building_module(_Builtin_stddef);
int declspec = __has_declspec_attribute(align);
#endif
int attribute = __has_c_attribute(fallthrough);
int scoped = ATTRIBUTE(gnu CAT(:,:) aligned);
#define VERSION __has_c_attribute(fallthrough)
#undef __has_c_attribute
#define __has_c_attribute(x) 17
int override = VERSION;
