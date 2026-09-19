#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <libgen.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile sig_atomic_t keep_running = 1;

static void stop_process(int signal_number) {
    (void)signal_number;
    keep_running = 0;
}

static void fail(const char *message) {
    perror(message);
    exit(EXIT_FAILURE);
}

static const char *identity_for_executable(const char *executable) {
    if (strcmp(executable, "blockuntu-test-app-a") == 0) {
        return "bk-test-app-a";
    }
    if (strcmp(executable, "blockuntu-test-app-b") == 0) {
        return "bk-test-app-b";
    }
    if (strcmp(executable, "blockuntu-test-block") == 0) {
        return "bk-test-block";
    }
    if (strcmp(executable, "blockuntu-test-parent") == 0) {
        return "bk-test-parent";
    }
    if (strcmp(executable, "blockuntu-test-helper") == 0) {
        return "bk-test-helper";
    }
    return "bk-test-unknown";
}

static unsigned int parse_lifetime(const char *value) {
    char *end = NULL;
    errno = 0;
    unsigned long parsed = strtoul(value, &end, 10);
    if (errno != 0 || end == value || *end != '\0' || parsed > 86400UL) {
        fprintf(stderr, "invalid --lifetime value: %s\n", value);
        exit(EXIT_FAILURE);
    }
    return (unsigned int)parsed;
}

static void write_ready_file(
    const char *path,
    const char *identity,
    const char *executable,
    pid_t child_pid
) {
    if (path == NULL) {
        return;
    }

    FILE *file = fopen(path, "w");
    if (file == NULL) {
        fail("fopen ready file");
    }
    if (fprintf(
            file,
            "{\"pid\":%ld,\"identity\":\"%s\",\"executable\":\"%s\",\"child_pid\":%ld}\n",
            (long)getpid(),
            identity,
            executable,
            (long)child_pid
        ) < 0) {
        fclose(file);
        fail("write ready file");
    }
    if (fclose(file) != 0) {
        fail("close ready file");
    }
}

int main(int argc, char **argv) {
    const char *ready_file = NULL;
    const char *helper_path = NULL;
    unsigned int lifetime_seconds = 600;

    for (int index = 1; index < argc; index += 1) {
        if (strcmp(argv[index], "--ready-file") == 0 && index + 1 < argc) {
            ready_file = argv[++index];
        } else if (strcmp(argv[index], "--spawn-helper") == 0 && index + 1 < argc) {
            helper_path = argv[++index];
        } else if (strcmp(argv[index], "--lifetime") == 0 && index + 1 < argc) {
            lifetime_seconds = parse_lifetime(argv[++index]);
        } else {
            fprintf(stderr, "unknown or incomplete argument: %s\n", argv[index]);
            return EXIT_FAILURE;
        }
    }

    char executable_copy[256];
    if (snprintf(executable_copy, sizeof(executable_copy), "%s", argv[0]) >=
        (int)sizeof(executable_copy)) {
        fprintf(stderr, "executable path is too long\n");
        return EXIT_FAILURE;
    }
    const char *executable_name = basename(executable_copy);
    const char *identity = identity_for_executable(executable_name);

    if (prctl(PR_SET_NAME, identity, 0, 0, 0) != 0) {
        fail("prctl PR_SET_NAME");
    }

    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = stop_process;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGTERM, &action, NULL) != 0 ||
        sigaction(SIGINT, &action, NULL) != 0 ||
        sigaction(SIGALRM, &action, NULL) != 0) {
        fail("sigaction");
    }

    pid_t child_pid = -1;
    if (helper_path != NULL) {
        child_pid = fork();
        if (child_pid < 0) {
            fail("fork helper");
        }
        if (child_pid == 0) {
            execl(helper_path, helper_path, "--lifetime", "600", (char *)NULL);
            fail("exec helper");
        }
    }

    write_ready_file(ready_file, identity, argv[0], child_pid);
    printf(
        "READY identity=%s pid=%ld child_pid=%ld\n",
        identity,
        (long)getpid(),
        (long)child_pid
    );
    fflush(stdout);

    if (lifetime_seconds > 0) {
        alarm(lifetime_seconds);
    }
    while (keep_running) {
        pause();
    }

    if (child_pid > 0) {
        kill(child_pid, SIGTERM);
        while (waitpid(child_pid, NULL, 0) < 0 && errno == EINTR) {
        }
    }

    printf("STOP identity=%s pid=%ld\n", identity, (long)getpid());
    fflush(stdout);
    return EXIT_SUCCESS;
}
