int grid[2][3][4];
int *element = &grid[1][2][3];
int *row = grid[1][2];
struct Object { int values[2][3]; }; struct Object object;
int *member = object.values[1];
int function(void);
int (*callback)(void) = *function;
