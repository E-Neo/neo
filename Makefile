CC = gcc -std=c11
CFLAGS = -Wall -Wextra -Wl,--gc-sections

MKDIR = mkdir -p
RM = rm -rf

SRC = src
SRC_C=$(shell find $(SRC) -type f -name '*.c')
OUTPUT = output

MAIN = $(OUTPUT)/main
MAIN_TEST = $(OUTPUT)/main_test

test: $(SRC_C)
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -I$(SRC) -O0 -g -o $(MAIN_TEST) $^

debug: $(SRC_C)
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -I$(SRC) -O0 -g -o $(MAIN) $^

release: $(SRC_C)
	$(MKDIR) $(OUTPUT)
	$(CC) $(CFLAGS) -I$(SRC) -O2 -o $(MAIN) $^

clean:
	$(RM) $(OUTPUT)
