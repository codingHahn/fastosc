CFLAGS = -O1 -g3 -I ../../target/include
LDLIBS = -ldl -lm

EXAMPLES = reciever simple_reciever reciever_with_data

all: $(EXAMPLES)

$(EXAMPLES): %: %.c ../../target/debug/libhiffi.so

../../target/debug/liblibhiffi.so:
	cd .. && cargo build

clean:
	rm -f $(EXAMPLES)

.PHONY: clean
