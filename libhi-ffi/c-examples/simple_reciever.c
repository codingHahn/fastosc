#include "libhi.h"
#include <stdbool.h>
#include <stdio.h>
#include <unistd.h>

void generic_callback(const char *address, const char *types,
                      const void *const *args, int32_t len, OscAnswer *answer,
                      const void *user_data) {
  printf("Got message for address %s, with type len %d types %s types\n",
         address, len, types);
}

int main(void) {

  // Create new server listening at 127.0.0.1 at port 50000 UDP
  struct OscServer *serv = hi_server_new("127.0.0.1:50000");
  if (serv == NULL) {
    printf("Creating server failed\n");
    return 1;
  }
  printf("Creating server succeeded, got ptr 0x%x\n", serv);

  // Register a handler at address "/test"
  enum ApiResult res =
      hi_register_handler(serv, "/test", "i", generic_callback, NULL);
  printf("Registered handler, API result: %d\n", res);

  // Start the thread handling OSC messages
  hi_start_thread(serv);

  // Never exit, because the osc thread would also die
  while (true) {
    sleep(5);
  }
  return 0;
}
