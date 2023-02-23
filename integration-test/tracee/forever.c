#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

void step() {
    void *mem = malloc(1024 * 1024);
    free(mem);

    printf("step\n");
    fflush(stdout);
}

int main() {
    while(1) {
        step();
        sleep(1);
    }

    return 0;
}
