#!/usr/bin/env python3
import hashlib,json,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
T=json.loads((ROOT/'timing-validated/capture.json').read_text())
A=json.loads((ROOT/'allocations-validated/capture.json').read_text())
assert T['status']==A['status']=='passed'
assert T['correctness_preflight_sha256']==A['correctness_preflight_sha256']
rows=[]
for name,summary in T['summary'].items():
    tr={v:[r for r in T['rows'] if r['workload']==name and r['variant']==v] for v in ['baseline','candidate']}
    ar={v:[r for r in A['rows'] if r['workload']==name and r['variant']==v] for v in ['baseline','candidate']}
    ratios=[p['latency_ratio'] for p in summary['pairs']]
    row={'workload':name,'timing_process_pairs':len(ratios),'allocation_process_pairs':len(ar['baseline']),
         'median_ms':{v:statistics.median(statistics.median(r['samples_ms']) for r in tr[v]) for v in tr},
         'paired_latency_ratio_median':statistics.median(ratios),'paired_latency_ratio_range':[min(ratios),max(ratios)],
         'normal_rss_kib_median':{v:statistics.median(r['peak_rss_kib'] for r in tr[v]) for v in tr},
         'paired_normal_rss_ratio_median':summary['median_ratios']['rss_ratio'],
         'paired_normal_rss_ratio_range':[min(p['rss_ratio'] for p in summary['pairs']),max(p['rss_ratio'] for p in summary['pairs'])],
         'allocation_calls_median':{},'requested_bytes_median':{},'allocation_counts_deterministic':True,
         'paired_allocation_calls_ratio_median':A['summary'][name]['median_ratios']['allocation_calls_ratio'],
         'paired_requested_bytes_ratio_median':A['summary'][name]['median_ratios']['requested_bytes_ratio']}
    for variant in ar:
        calls=[sample[0] for r in ar[variant] for sample in r['allocations']]
        requested=[sample[1] for r in ar[variant] for sample in r['allocations']]
        row['allocation_calls_median'][variant]=statistics.median(calls)
        row['requested_bytes_median'][variant]=statistics.median(requested)
        row['allocation_counts_deterministic'] &= len(set(calls))==len(set(requested))==1
    rows.append(row)
result={'status':'passed','timing_process_count':len(T['rows']),'timed_calls':sum(len(r['samples_ms']) for r in T['rows']),
        'allocation_process_count':len(A['rows']),'instrumented_calls':sum(len(r['allocations']) for r in A['rows']),
        'all_latency_pairs_improved':all(r['paired_latency_ratio_range'][1]<1 for r in rows),
        'output_hash_failures':0,'exact_parser_spans':T['exact_parser_spans'],'exact_checked_outputs':T['exact_checked_outputs'],
        'nonwhitespace_fix_count':T['nonwhitespace_fix_count'],'rows':rows,
        'qualification':'Ratios compare the two process medians within each round. Displayed milliseconds are separate medians across process medians. Pair ranges are observed min/max, not confidence intervals. System allocations exclude snapshot verification/destruction. RSS is whole process. Shared x86_64 VM; no local builds/probes/fuzzing during capture.'}
(ROOT/'measurement-summary.json').write_text(json.dumps(result,indent=2)+'\n')
for r in rows:
    delta=100*(r['paired_latency_ratio_median']-1)
    lo,hi=[100*(x-1) for x in r['paired_latency_ratio_range']]
    calls=100*(r['paired_allocation_calls_ratio_median']-1);requested=100*(r['paired_requested_bytes_ratio_median']-1);rss=100*(r['paired_normal_rss_ratio_median']-1)
    print(f"{r['workload']:23} {r['median_ms']['baseline']:8.3f} -> {r['median_ms']['candidate']:8.3f} ms; latency {delta:+6.1f}% [{lo:+.1f}, {hi:+.1f}]; calls {calls:+.1f}%; bytes {requested:+.1f}%; RSS {rss:+.1f}%")
print(json.dumps({k:v for k,v in result.items() if k!='rows'}))
