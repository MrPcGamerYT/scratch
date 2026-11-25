/* Nexus guest bridge v1. Private virtio port, fixed commands only; no shell API.
 * This is a prototype privileged service, not a security certification. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <ctype.h>
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
#define MAX_APK (512ULL*1024*1024)
static char apk_path[128];
static char apk_digest[65];

static int command(const char *line, const char **name, const char **key) {
    struct entry { const char *line, *name, *key; };
    static const struct entry entries[] = {
        {"NEXUS 1 BACK", "BACK", "4"}, {"NEXUS 1 HOME", "HOME", "3"},
        {"NEXUS 1 RECENTS", "RECENTS", "187"}, {"NEXUS 1 VOLUME_UP", "VOLUME_UP", "24"},
        {"NEXUS 1 VOLUME_DOWN", "VOLUME_DOWN", "25"}, {"NEXUS 1 MUTE", "MUTE", "164"},
        {"NEXUS 1 PROVISION_HOME", "PROVISION_HOME", "PROVISION_HOME"},
        {"NEXUS 1 QUERY_APP", "QUERY_APP", "QUERY_APP"},
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
        if (key && !strcmp(key,"PROVISION_HOME")) execl("/system/bin/sh", "sh", access("/nexus-provision-home.sh",R_OK)==0 ? "/nexus-provision-home.sh" : "/system/bin/nexus-provision-home.sh", (char *)NULL);
        else if (key && !strcmp(key,"CHECK_HOME")) execl("/system/bin/sh", "sh", access("/nexus-home-check.sh",R_OK)==0 ? "/nexus-home-check.sh" : "/system/bin/nexus-home-check.sh", (char *)NULL);
        else if (key && !strcmp(key,"APK_INSTALL")) execl("/system/bin/pm", "pm", "install", "-r", "--user", "0", apk_path, (char *)NULL);
        else if (key) execl("/system/bin/input", "input", "keyevent", key, (char *)NULL);
        else execl("/system/bin/svc", "svc", "power", "shutdown", (char *)NULL);
        _exit(127);
    }
    int budget = key && !strcmp(key,"PROVISION_HOME") ? 1200 : (key && !strcmp(key,"APK_INSTALL") ? 1200 : 600);
    for (int i = 0; i < budget; ++i) {
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

/* Never accept a package or shell command from the peer for app discovery. */
static int app_package(const char *text, char *package, size_t size) {
    if (!strstr(text,"mResumedActivity") && !strstr(text,"topResumedActivity")) return 0;
    const char *slash = strchr(text,'/');
    if (!slash) return 0;
    const char *start = slash;
    while (start > text && (isalnum((unsigned char)start[-1]) || start[-1]=='_' || start[-1]=='.')) --start;
    size_t length = (size_t)(slash-start);
    if (!length || length >= size || !memchr(start,'.',length)) return 0;
    memcpy(package,start,length); package[length]=0; return 1;
}

static void run_command(int output, const char *name, const char *key) {
    int result;
    if (!strcmp(name,"QUERY_APP")) {
        char package[96] = "-", text[4096];
#ifndef NEXUS_AGENT_TEST
        FILE *source=popen("/system/bin/dumpsys activity activities", "r");
        result=-1;
        if (source) {
            int found=0;
            while (fgets(text,sizeof(text),source)) {
                if (!found && app_package(text,package,sizeof(package))) found=1;
            }
            result=pclose(source)==0 ? 0 : -1;
        }
#else
        (void)text;
        const char *fixture=getenv("NEXUS_TEST_ACTIVITY");
        if (fixture) app_package(fixture,package,sizeof(package));
        result=0;
#endif
        char event[128]; snprintf(event,sizeof(event),"NEXUS 1 APP %s\n",result==0 ? package : "-");
        reply(output,event);
    } else if (!strcmp(name,"APK_INSTALL")) {
        result=-1;
#ifndef NEXUS_AGENT_TEST
        int ends[2];
        if (!pipe(ends)) {
            pid_t child=fork();
            if (!child) { close(ends[0]); dup2(ends[1],1); close(ends[1]); execl("/system/bin/sha256sum","sha256sum",apk_path,(char *)NULL); _exit(127); }
            close(ends[1]); char hash[65]={0}; size_t used=0;
            while (used<64) { ssize_t count=read(ends[0],hash+used,64-used); if (count<=0) break; used+=(size_t)count; }
            close(ends[0]); int status=0;
            if (child>0 && waitpid(child,&status,0)==child && WIFEXITED(status) && WEXITSTATUS(status)==0 && used==64 && !strcmp(hash,apk_digest)) result=execute("APK_INSTALL");
        }
#endif
        unlink(apk_path);
    } else result=execute(key);
    char response[96];
    snprintf(response,sizeof(response),"NEXUS 1 RESULT %s %s\n",name,result==0 ? "OK" : "FAILED");
    reply(output,response);
}

static int serve_loop(int input, int output, int testing, pid_t *worker, int *workfd, int *apkfd, int *apkowned) {
    char line[MAX_LINE]; size_t used = 0; int dropping = 0;
    time_t next_status = 0, apk_activity=0;
    unsigned long long apk_total=0, apk_received=0;
    size_t apk_remaining=0;
    if (reply(output,"NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK\n")) return -1;
    time_t worker_started=0;
    for (;;) {
        if (*apkowned && apk_activity && time(NULL)-apk_activity>180) return -1;
        if (*worker>0) {
            char event[256];
            ssize_t count=read(*workfd,event,sizeof(event)-1);
            if (count>0) { event[count]=0; if (reply(output,event)) return -1; }
            int status=0;
            if (waitpid(*worker,&status,WNOHANG)==*worker) {
                while ((count=read(*workfd,event,sizeof(event)-1))>0) {
                    event[count]=0; if (reply(output,event)) return -1;
                }
                close(*workfd); *workfd=-1; *worker=0;
                if (*apkowned && *apkfd<0) *apkowned=0;
            } else if (time(NULL)-worker_started>130) {
                kill(-*worker,SIGKILL); waitpid(*worker,NULL,0);
                close(*workfd); *workfd=-1; *worker=0;
                if (*apkowned && *apkfd<0) *apkowned=0;
                if (reply(output,"NEXUS 1 ERROR COMMAND_TIMEOUT\n")) return -1;
            }
        }
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
        if (apk_remaining) {
            char chunk[16384];
            ssize_t count=read(input,chunk,apk_remaining<sizeof(chunk) ? apk_remaining : sizeof(chunk));
            if (!count) return -1;
            if (count<0) { if (errno==EINTR || errno==EAGAIN) continue; return -1; }
            size_t position=0;
            while (position<(size_t)count) {
                ssize_t written=write(*apkfd,chunk+position,(size_t)count-position);
                if (written<0 && errno==EINTR) continue;
                if (written<=0) return -1;
                position+=(size_t)written;
            }
            apk_remaining-=(size_t)count; apk_received+=(size_t)count; apk_activity=time(NULL);
            if (!apk_remaining && reply(output,"NEXUS 1 APK NEXT\n")) return -1;
            continue;
        }
        char byte;
        ssize_t count = read(input,&byte,1);
        if (count == 0) return 0;
        if (count < 0) { if (errno == EINTR || errno == EAGAIN) continue; return -1; }
        if (byte == '\n') {
            if (dropping) { reply(output,"NEXUS 1 ERROR OVERSIZE\n"); }
            else {
                line[used] = 0;
                const char *name = NULL, *key = NULL;
                int install=0;
                if (!strncmp(line,"NEXUS 1 APK BEGIN ",18)) {
                    unsigned long long total=0; char digest[65],extra;
                    int parsed=sscanf(line+18,"%llu %64s %c",&total,digest,&extra);
                    int valid=parsed==2 && strlen(digest)==64;
                    if (valid) for (int i=0;i<64;i++) if (!((digest[i]>='0' && digest[i]<='9') || (digest[i]>='a' && digest[i]<='f'))) valid=0;
                    if (!valid || !total || total>MAX_APK || *apkowned || *worker>0 || testing) {
                        if (reply(output,"NEXUS 1 APK FAILED\n")) return -1;
                    } else {
                        *apkfd=open(apk_path,O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW|O_CLOEXEC,0600);
                        if (*apkfd<0) { if (reply(output,"NEXUS 1 APK FAILED\n")) return -1; }
                        else {
                            *apkowned=1; apk_total=total; apk_received=0;
                            memcpy(apk_digest,digest,65); apk_activity=time(NULL);
                            if (reply(output,"NEXUS 1 APK NEXT\n")) return -1;
                        }
                    }
                    used=0; dropping=0; continue;
                } else if (!strncmp(line,"NEXUS 1 APK DATA ",17)) {
                    unsigned long long length=0; char extra;
                    if (sscanf(line+17,"%llu %c",&length,&extra)!=1 || !*apkowned || *apkfd<0 || !length || length>65536 || length>apk_total-apk_received) return -1;
                    apk_remaining=(size_t)length; used=0; dropping=0; continue;
                } else if (!strcmp(line,"NEXUS 1 APK COMMIT")) {
                    if (!*apkowned || *apkfd<0 || apk_received!=apk_total || *worker>0) return -1;
                    if (fsync(*apkfd)) return -1;
                    close(*apkfd); *apkfd=-1; apk_activity=0;
                    name="APK_INSTALL"; key="APK_INSTALL"; install=1;
                }
                if (install || command(line,&name,&key)) {
                    if (testing) run_command(output,name,key);
                    else if (*worker>0) { if (reply(output,"NEXUS 1 ERROR BUSY\n")) return -1; }
                    else {
                        int ends[2];
                        if (pipe(ends)) return -1;
                        *worker=fork();
                        if (*worker<0) { close(ends[0]); close(ends[1]); return -1; }
                        if (!*worker) {
                            setpgid(0,0); close(ends[0]); close(input);
                            if (output!=input) close(output);
                            run_command(ends[1],name,key); close(ends[1]); _exit(0);
                        }
                        setpgid(*worker,*worker); close(ends[1]);
                        *workfd=ends[0]; fcntl(*workfd,F_SETFL,O_NONBLOCK);
                        worker_started=time(NULL);
                    }
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

static int serve(int input, int output, int testing) {
    pid_t worker=0; int workfd=-1,apkfd=-1,apkowned=0;
    snprintf(apk_path,sizeof(apk_path),"/data/local/tmp/nexus-install-%ld.apk",(long)getpid());
    int result=serve_loop(input,output,testing,&worker,&workfd,&apkfd,&apkowned);
    if (worker>0) { kill(-worker,SIGKILL); waitpid(worker,NULL,0); }
    if (workfd>=0) close(workfd);
    if (apkfd>=0) close(apkfd);
    if (apkowned) unlink(apk_path);
    return result;
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
