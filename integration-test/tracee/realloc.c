#include <stdlib.h>

int main() {
    void *mem = malloc(1);

    for (int i = 0; i < 20; i++) {
	mem = realloc(mem, 1 << i);
    }

    free(mem);

    return 0;
}
