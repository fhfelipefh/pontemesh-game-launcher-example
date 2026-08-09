#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <string>

#include "launcher_core.hpp"
#include "native_sdk.hpp"

namespace {

std::string trim(std::string value) {
    const auto first = value.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return {};
    const auto last = value.find_last_not_of(" \t\r\n");
    value = value.substr(first, last - first + 1);
    if (value.size() >= 2 && value.front() == '"' && value.back() == '"') {
        value = value.substr(1, value.size() - 2);
    }
    return value;
}

std::map<std::string, std::string> load_config(const std::filesystem::path& path) {
    std::ifstream input(path);
    if (!input) throw std::runtime_error("could not read " + path.string());
    std::map<std::string, std::string> config;
    for (std::string line; std::getline(input, line);) {
        const auto separator = line.find('=');
        if (separator != std::string::npos) config[trim(line.substr(0, separator))] = trim(line.substr(separator + 1));
    }
    if (const char* value = std::getenv("PONTEMESH_ORIGIN_URL")) config["origin_url"] = value;
    if (const char* value = std::getenv("PONTEMESH_APPLICATION_TOKEN")) config["application_token"] = value;
    for (const auto* name : {"origin_url", "application_token", "release_bucket", "release_manifest_key"}) {
        if (config[name].empty()) throw std::runtime_error(std::string(name) + " cannot be empty");
    }
    if (config["origin_url"].rfind("http://", 0) != 0 && config["origin_url"].rfind("https://", 0) != 0) {
        throw std::runtime_error("origin_url must use HTTP or HTTPS");
    }
    return config;
}

void merge(PontemeshTransferSummary& total, const PontemeshTransferSummary& current) {
    total.bytes_from_peer += current.bytes_from_peer;
    total.bytes_from_replica += current.bytes_from_replica;
    total.bytes_from_origin += current.bytes_from_origin;
    total.fragments_from_peer += current.fragments_from_peer;
    total.fragments_from_replica += current.fragments_from_replica;
    total.fragments_from_origin += current.fragments_from_origin;
    total.peer_failures += current.peer_failures;
    total.peer_hash_failures += current.peer_hash_failures;
    total.peer_rejected_fragments += current.peer_rejected_fragments;
    total.fallback_activations += current.fallback_activations;
}

}  // namespace

int main() {
    try {
        const std::filesystem::path repo_root = PONTEMESH_REPO_ROOT;
        const auto config = load_config(repo_root / "launcher.toml");
        const auto install_root = repo_root / "runtime" / "installations" / "cpp";
        std::filesystem::create_directories(install_root.parent_path());
        const auto unique = std::chrono::steady_clock::now().time_since_epoch().count();
        const auto work = install_root.parent_path() / (".pontemesh-cpp-" + std::to_string(unique));
        std::filesystem::create_directories(work);
        pontemesh::Client client(pontemesh::find_library(repo_root), config.at("origin_url"), config.at("application_token"));
        PontemeshTransferSummary total{};
        const auto descriptor = work / "release.json";
        client.sync_object(config.at("release_bucket"), config.at("release_manifest_key"), descriptor);
        const auto release = launcher::validate_manifest(launcher::read_json(descriptor));
        const auto staging = work / "installation";
        std::filesystem::create_directories(staging);
        for (const auto& file : release.files) {
            const auto destination = staging / file.path;
            auto summary = client.sync_object(file.bucket, file.key, destination, [&file](auto fragment, auto downloaded, auto size, const auto& source) {
                const auto percent = size ? downloaded * 100 / size : 100;
                std::cout << file.path.generic_string() << ": " << percent << "% (fragment " << fragment + 1 << ", " << source << ")\n";
            });
            if (std::filesystem::file_size(destination) != file.size_bytes) {
                throw std::runtime_error("release size verification failed for " + file.path.string());
            }
            merge(total, summary);
        }
        std::ofstream(staging / ".pontemesh-version") << release.version << '\n';
        launcher::replace_installation(install_root, staging);
        std::filesystem::remove_all(work);
        std::cout << "\nUpdate " << release.version << " installed in " << install_root << '\n';
        std::cout << "Origin: " << total.bytes_from_origin << " bytes; Replica/Edge: " << total.bytes_from_replica << "; Peers: " << total.bytes_from_peer << '\n';
        std::cout << "Game status: READY TO PLAY\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << "Update failed: " << error.what() << '\n';
        return 1;
    }
}
