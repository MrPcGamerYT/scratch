#include <cstdlib>
#include <fstream>
#include <string>

// Launcher contract test fixture, not an emulator.
int main(int argc, char **argv) {
    const char *logPath = std::getenv("NEXUS_TEST_LOG");
    if (!logPath) return 2;
    std::ofstream log(logPath, std::ios::app);
    if (!log) return 3;
    log << "BEGIN\n";
    for (int i = 1; i < argc; ++i) log << argv[i] << '\n';
    log << "END\n";
    if (argc == 6 && std::string(argv[1]) == "create") {
        std::ofstream disk(argv[4]);
        if (!disk) return 4;
        disk << "stub qcow2";
    }
    return 0;
}
