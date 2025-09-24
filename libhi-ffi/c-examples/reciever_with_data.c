#include "libhi.h"
#include <stdbool.h>
#include <stdio.h>
#include <unistd.h>

// This example is meant to showcase how to use the arguments (data) that get
// sent over OSC

// Considerations:
// For the sake of this simple example, I just use a global variable to save the
// state. Please consider that you need a thread safe way of accessing
// user_data, since OSC is running in its own thread. A mutex would be perfect
// for this.
int save_int = 0;
float save_float = 0.0;

// The callback gets passed `user_data` as the last argument. It is of type
// `void *`, so it needs to be cast into the right type again for access.
void save_int_callback(const char *address, const char *types,
                       const SocketAddr *socket, const void *const *args,
                       int32_t len, const void *user_data, OscAnswer *answer) {
  printf("Got message for address %s, with type len %d types %s types\n",
         address, len, types);

  // Cast into right pointer type before dereferencing
  save_int = ((int *)(args))[0];
}

void save_float_callback(const char *address, const char *types,
                         const SocketAddr *socket, const void *const *args,
                         int32_t len, const void *user_data,
                         OscAnswer *answer) {
  printf("Got message for address %s, with type len %d types %s types\n",
         address, len, types);

  // Cast into right pointer type before dereferencing
  save_float = ((int *)(args))[0];
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
      hi_register_handler(serv, "/test", "i", save_int_callback, NULL);
  printf("Registered handler, API result: %d\n", res);

  res = hi_register_handler(serv, "/test2", "f", save_float_callback, NULL);
  printf("Registered handler, API result: %d\n", res);

  // Start the thread handling OSC messages
  hi_start_thread(serv);

  // Never exit, because the osc thread would also die
  while (true) {
    sleep(5);
  }
  return 0;
}
