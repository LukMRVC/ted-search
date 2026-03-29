#include <algorithm>
#include <unistd.h>
#include <ctime>
#include <sys/resource.h>
#include <chrono>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <unordered_set>
#include <utility>
#include <vector>
#include "node.h"
#include "string_label.h"
#include "unit_cost_model.h"
#include "ted_algorithm_touzet.h"
#include "bracket_notation_parser.h"
#include "touzet_kr_set_tree_index.h"
#include "touzet_baseline_tree_index.h"

rlim_t increase_stack_size(rlim_t &);

using namespace std::chrono;

std::vector<std::pair<int, int>> read_candidates(std::string &candidates_path);
std::vector<std::pair<int, std::string>>
read_query_file(std::string &query_path);

// Hash function that treats (a,b) and (b,a) as equivalent
struct SymmetricPairHash {
  std::size_t operator()(const std::pair<int, int> &p) const {
    // Use the smaller number first to ensure (a,b) and (b,a) hash the same
    auto small = std::min(p.first, p.second);
    auto large = std::max(p.first, p.second);
    // Combine hashes - order doesn't matter now since we sorted the values
    return std::hash<int>{}(small) ^ (std::hash<int>{}(large) << 1);
  }
};

// Equality function that treats (a,b) and (b,a) as equivalent
struct SymmetricPairEqual {
  bool operator()(const std::pair<int, int> &p1,
                  const std::pair<int, int> &p2) const {
    return (p1.first == p2.first && p1.second == p2.second) ||
           (p1.first == p2.second && p1.second == p2.first);
  }
};

struct PairHash {
  inline std::size_t operator()(const std::pair<int, int> &v) const {
    return v.first * 300 + v.second;
  }
};

struct VerificationResult {
  int query_id;
  int candidate_id;
  int16_t passed;
  long verification_time;
  double final_ted;
};

int main(int argc, char *argv[]) {
  std::ios::sync_with_stdio(false);
  if (argc < 4) {
    std::cerr << "Missing arguments for DATASET, QUERY_FILE, CANDIDATES*"
              << std::endl;
    exit(1);
  }

  std::vector<std::string> args(argv + 1, argv + argc);

  // path to dataset file containing trees in BN (Bracket notation)
  std::string dataset_path = args.at(0);
  std::string query_path = args.at(1);

  // we can have N candidate files, we merge all candidate pairs into single set
  std::unordered_set<std::pair<int, int>, PairHash> candidate_set;
  for (size_t i = 2; i < args.size(); ++i) {
    auto candidates_vec = read_candidates(args[i]);
    candidate_set.insert(candidates_vec.begin(), candidates_vec.end());
  }

  // now increase stack size
  rlim_t new_stack_size = 16777216;
  if (increase_stack_size(new_stack_size) != new_stack_size) {
    std::cerr << "Unable to increase stack size!" << std::endl;
    exit(1);
  }

  auto queries = read_query_file(query_path);

  using Label = label::StringLabel;
  using CostModelLD = cost_model::UnitCostModelLD<Label>;
  using LabelDictionary = label::LabelDictionary<Label>;
  //  Use APTED to compute actual tree distances at least once between all trees
  //  <- this is going to take a lot of time Computing all these distances is
  //  necessary, so we can determine the success rate of the filter The
  //  computation is going to take a lot of time, might as well run it in
  //  parallel :]
  using TedTouzet =
      ted::TouzetKRSetTreeIndex<CostModelLD, node::TreeIndexTouzetKRSet>;

  // PARSE INPUT
  // The input is parsed once for the entire experiment.
  std::vector<node::Node<Label>> trees_collection;
  parser::BracketNotationParser<Label> bnp;
  std::cerr << "Parsing tree collection for " << dataset_path << std::endl;
  bnp.parse_collection(trees_collection, dataset_path);
  std::cerr << "Parsing done" << std::endl;

  // std::cout << trees_collection[1099] << std::endl;

  std::vector<std::pair<unsigned long, node::Node<Label>>> query_collection;

  for (const auto &[t, tstr] : queries) {
    auto query_tree = bnp.parse_single(tstr);
    query_collection.emplace_back(t, query_tree);
  }
  //    std::vector<std::pair<int, int>> candidates2vec =
  //    read_candidates(candidates2);

  LabelDictionary ld;
  CostModelLD ucm(ld);
  std::cerr << "Parsed " << trees_collection.size() << " trees" << std::endl;
  int i = 0;

  std::unordered_set<std::pair<int, int>, SymmetricPairHash, SymmetricPairEqual>
      computed_pairs;

  std::vector<node::TreeIndexTouzetKRSet> tree_indexes;
  for (size_t j = 0; j < trees_collection.size(); ++j) {
    node::TreeIndexTouzetKRSet ti;
    node::index_tree(ti, trees_collection[j], ld, ucm);
    tree_indexes.emplace_back(ti);
  }

  std::vector<std::pair<int, int>> candidate_vector(candidate_set.begin(),
                                                    candidate_set.end());

  std::vector<VerificationResult> verification_results;
  verification_results.reserve(candidate_vector.size());

  auto total_start = high_resolution_clock::now();
  for (const auto &[t1, t2] : candidate_vector) {
    node::TreeIndexTouzetKRSet ti;
    auto [threshold, query_tree] = query_collection[t1];
    node::index_tree(ti, query_tree, ld, ucm);

    TedTouzet top_diff(ucm);

    auto compute_ted_duration = [&]() {
      auto start = high_resolution_clock::now();
      auto final_ted = top_diff.ted_k(ti, tree_indexes[t2], threshold);
      auto stop = high_resolution_clock::now();
      auto duration =
          std::chrono::duration_cast<microseconds>(stop - start).count();
      return std::make_pair(final_ted, duration);
    };

    auto [final_ted, duration0] = compute_ted_duration();
    auto [_, duration1] = compute_ted_duration();
    auto [__, duration2] = compute_ted_duration();

    auto duration = std::min({duration0, duration1, duration2});
    {
      verification_results.push_back(
          {t1, t2, final_ted <= threshold, duration, final_ted});

      i += 1;
      //   std::cerr << "[" << t1 << ", " << t2 << "]" << std::endl;
      //   if (i % 1'000 == 0) {
      // std::cerr << i << std::endl;
      //   }
    }
  }
  auto total_stop = high_resolution_clock::now();
  auto total_duration =
      std::chrono::duration_cast<milliseconds>(total_stop - total_start)
          .count();

  std::cerr << "Printing verification results" << std::endl;
  std::cout
      << "query_id,candidate_id,verification_time_micro,final_ted,passed\n";
  for (const auto vr : verification_results) {
    std::cout << vr.query_id << "," << vr.candidate_id << ","
              << vr.verification_time << "," << vr.final_ted << "," << vr.passed
              << "\n";
  }
  std::cout << "-1;-1;" << total_duration << ";-1;-1\n"; // end of file marker

  return 0;
}

std::vector<std::pair<int, int>> read_candidates(std::string &candidates_path) {
  std::vector<std::pair<int, int>> candidatesvec;

  std::string line;
  std::string tmp;
  std::ifstream results(candidates_path);
  size_t t1;
  size_t t2;

  std::vector<std::vector<size_t>> results_map;

  while (std::getline(results, line)) {
    auto ls = std::stringstream(line);
    std::getline(ls, tmp, ',');
    t1 = std::stoul(tmp);

    std::getline(ls, tmp, '\n');
    t2 = std::stoul(tmp);

    candidatesvec.emplace_back(t1, t2);
  }

  return candidatesvec;
}

std::vector<std::pair<int, std::string>>
read_query_file(std::string &query_path) {
  std::ifstream cfile(query_path);
  std::vector<std::pair<int, std::string>> candidatesvec;
  std::string line;
  std::string tmp;
  size_t t1;
  size_t t2;

  std::vector<std::vector<size_t>> results_map;

  while (std::getline(cfile, line)) {
    auto ls = std::stringstream(line);
    std::getline(ls, tmp, ';');
    t1 = std::stoul(tmp);

    std::getline(ls, tmp, '\n');
    candidatesvec.emplace_back(t1, tmp);
  }

  return candidatesvec;
}

rlim_t increase_stack_size(rlim_t &new_stack_size) {
  // Define and object of structure rlimit.
  struct rlimit rl;
  // First get the time limit on CPU
  getrlimit(RLIMIT_STACK, &rl);
  // printf("\n Default value is : %lld\n", (long long int)rl.rlim_cur);
  // Change the time limit
  rl.rlim_cur = new_stack_size;
  // Now call setrlimit() to set the
  // changed value.
  setrlimit(RLIMIT_STACK, &rl);
  // Again get the limit and check
  getrlimit(RLIMIT_STACK, &rl);
  // printf("\n Default value now is : %lld\n", (long long int)rl.rlim_cur);
  return rl.rlim_cur;
}
