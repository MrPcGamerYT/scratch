/* Nexus guest bridge v1. Private virtio port, fixed commands only; no shell API.
 * This is a prototype privileged service, not a security certification. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <glob.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define PORT_NAME "org.nexus.control.0"
#define MAX_LINE 128

static int command(const char *line, const char **name, const char **key) {
    struct entry { const char *line, *name, *key; };
    static const struct entry entries[] = {
        {"NEXUS 1 BACK", "BACK", "4"}, {"NEXUS 1 HOME", "HOME", "3"},
        {"NEXUS 1 RECENTS", "RECENTS", "187"}, {"NEXUS 1 VOLUME_UP", "VOLUME_UP", "24"},
        {"NEXUS 1 VOLUME_DOWN", "VOLUME_DOWN", "25"}, {"NEXUS 1 MUTE", "MUTE", "164"},
        {"NEXUS 1 CHECK_HOME", "CHECK_HOME", "CHECK_HOME"},
        {"NEXUS 1 SHUTDOWN", "SHUTDOWN", NULL}
    };
    for (size_t i = 0; i < sizeof(entries)/sizeof(entries[0]); ++i) {
        if (!strcmp(line, entries[i].line)) { *name = entries[i].name; *key = entries[i].key; return 1; }
    }
    return 0;
}

static int execute(const char *key) {
#ifdef NEXUS_AGENT_TEST
    (void)key; return 0;
#else
    pid_t child = fork();
    if (child < 0) return -1;
    if (!child) {
        int nullfd = open("/dev/null", O_RDWR);
        if (nullfd >= 0) { dup2(nullfd,0); dup2(nullfd,1); dup2(nullfd,2); if (nullfd > 2) close(nullfd); }
        if (key && !strcmp(key,"CHECK_HOME")) execl("/system/bin/sh", "sh", access("/nexus-home-check.sh",R_OK)==0 ? "/nexus-home-check.sh" : "/system/bin/nexus-home-check.sh", (char *)NULL);
        else if (key) execl("/system/bin/input", "input", "keyevent", key, (char *)NULL);
        else execl("/system/bin/svc", "svc", "power", "shutdown", (char *)NULL);
        _exit(127);
    }
    for (int i = 0; i < 150; ++i) {
        int status = 0;
        pid_t result = waitpid(child, &status, WNOHANG);
        if (result == child) return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 0 : -1;
        if (result < 0 && errno != EINTR) return -1;
        struct timespec pause = {0,100000000}; nanosleep(&pause,NULL);
    }
    kill(child, SIGKILL); waitpid(child,NULL,0); return -1;
#endif
}

static int reply(int fd, const char *text) {
    size_t size = strlen(text);
    while (size) {
        ssize_t written = write(fd, text, size);
        if (written < 0 && errno == EINTR) continue;
        if (written <= 0) return -1;
        text += written; size -= (size_t)written;
    }
    return 0;
}

static int serve(int input, int output, int testing) {
    char line[MAX_LINE]; size_t used = 0; int dropping = 0;
    time_t next_status = 0;
    if (reply(output,"NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK\n")) return -1;
    for (;;) {
        if (!testing && time(NULL) >= next_status) {
            char value[32] = {0};
            FILE *property = popen("/system/bin/getprop sys.boot_completed", "r");
            if (property) { if (!fgets(value,sizeof(value),property)) value[0] = 0; pclose(property); }
            if (reply(output, value[0]=='1' ? "NEXUS 1 READY 1\n" : "NEXUS 1 READY 0\n")) return -1;
            next_status = time(NULL)+2;
        }
        struct pollfd p = {input,POLLIN,0};
        int status = poll(&p,1,1000);
        if (status < 0 && errno == EINTR) continue;
        if (status < 0 || (p.revents & (POLLERR|POLLNVAL))) return -1;
        if (!status) continue;
        if (!(p.revents & POLLIN)) { if (p.revents & POLLHUP) return 0; continue; }
        char byte;
        ssize_t count = read(input,&byte,1);
        if (count == 0) return 0;
        if (count < 0) { if (errno == EINTR || errno == EAGAIN) continue; return -1; }
        if (byte == '\n') {
            if (dropping) { reply(output,"NEXUS 1 ERROR OVERSIZE\n"); }
            else {
                line[used] = 0;
                const char *name = NULL, *key = NULL;
                if (command(line,&name,&key)) {
                    char response[96];
                    snprintf(response,sizeof(response),"NEXUS 1 RESULT %s %s\n",name,execute(key)==0 ? "OK" : "FAILED");
                    if (reply(output,response)) return -1;
                } else if (reply(output,"NEXUS 1 ERROR UNSUPPORTED\n")) return -1;
            }
            used = 0; dropping = 0;
        } else if (byte < 32 || byte > 126) { dropping = 1; }
        else if (!dropping) {
            if (used+1 >= sizeof(line)) dropping = 1;
            else line[used++] = byte;
        }
    }
}

#ifndef NEXUS_AGENT_TEST
static int open_port(void) {
    glob_t paths;
    if (glob("/sys/class/virtio-ports/*/name",0,NULL,&paths)) return -1;
    int fd = -1;
    for (size_t i=0;i<paths.gl_pathc;i++) {
        char name[80] = {0};
        FILE *file = fopen(paths.gl_pathv[i],"r");
        if (!file) continue;
        if (!fgets(name,sizeof(name),file)) name[0] = 0;
        fclose(file); name[strcspn(name,"\r\n")] = 0;
        if (strcmp(name,PORT_NAME)) continue;
        char path[256];
        const char *part = paths.gl_pathv[i]+strlen("/sys/class/virtio-ports/");
        size_t length = strcspn(part,"/");
        if (length > 64) continue;
        snprintf(path,sizeof(path),"/dev/%.*s",(int)length,part);
        fd = open(path,O_RDWR|O_CLOEXEC|O_NONBLOCK); break;
    }
    globfree(&paths); return fd;
}
#endif

int main(void) {
    signal(SIGPIPE,SIG_IGN);
#ifdef NEXUS_AGENT_TEST
    return serve(STDIN_FILENO,STDOUT_FILENO,1) < 0;
#else
    for (;;) {
        int fd = open_port();
        if (fd >= 0) { serve(fd,fd,0); close(fd); }
        sleep(2);
    }
#endif
}
