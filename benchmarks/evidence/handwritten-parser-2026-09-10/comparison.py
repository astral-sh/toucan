"""Narrow, auditable equivalence rules for successful parser snapshots."""
import hashlib,json,re
SPAN = re.compile(r'\bspan: ([0-9]+)…([0-9]+)')

def ast_ranges(ast):
    """Find Debug span fields outside quoted AST string values."""
    ranges=[];pieces=[];cursor=0;position=0
    while position<len(ast):
        if ast[position]=='"':
            position+=1
            while position<len(ast):
                if ast[position]=='\\': position+=2
                elif ast[position]=='"': position+=1;break
                else: position+=1
            continue
        match=SPAN.match(ast,position)
        if match:
            pieces.extend([ast[cursor:position],'span: <reviewed-range>'])
            ranges.append(tuple(map(int,match.groups())))
            position=cursor=match.end()
        else:position+=1
    pieces.append(ast[cursor:])
    return ''.join(pieces),ranges

def compare_parser(baseline,candidate,source):
    if baseline.get('outcome')!='ok' or candidate.get('outcome')!='ok':
        return {'accepted':False,'kind':'non_successful_parser_result'}
    if {k:v for k,v in baseline.items() if k!='ast'}!={k:v for k,v in candidate.items() if k!='ast'}:
        return {'accepted':False,'kind':'non_ast_fields_differ'}
    if baseline['source_bytes']!=len(source):
        return {'accepted':False,'kind':'source_length_mismatch'}
    before,left=ast_ranges(baseline['ast']);after,right=ast_ranges(candidate['ast'])
    if before!=after or len(left)!=len(right):
        return {'accepted':False,'kind':'non_span_ast_fields_differ'}
    changes=[]
    for index,(old,new) in enumerate(zip(left,right)):
        if old==new:continue
        start,end=old;new_start,new_end=new
        if not (start==new_start and 0<=start<=new_end<end<=len(source)):
            return {'accepted':False,'kind':'span_change_is_not_a_suffix_trim','index':index,'old':old,'new':new}
        suffix=source[new_end:end]
        if not suffix.isspace():
            return {'accepted':False,'kind':'span_removed_non_whitespace','index':index,'old':old,'new':new,'removed_bytes_hex':suffix.hex()}
        changes.append({'index':index,'old':old,'new':new,'removed_bytes_hex':suffix.hex()})
    return {'accepted':True,'kind':'trailing_whitespace_spans' if changes else 'exact_ast','exact_spans':not changes,'changed_span_count':len(changes),'changes':changes}

def diagnose_json(baseline,candidate):
    """Describe mismatches without granting equivalence for checked output."""
    changes=[]
    def visit(left,right,path):
        if type(left) is not type(right):
            changes.append({'path':path,'kind':'type','baseline':left,'candidate':right});return
        if isinstance(left,dict):
            for key in sorted(left.keys()|right.keys()):
                if key not in left or key not in right:
                    changes.append({'path':path+[key],'kind':'missing_key','baseline_has_key':key in left,'candidate_has_key':key in right})
                else:visit(left[key],right[key],path+[key])
        elif isinstance(left,list):
            if len(left)!=len(right):changes.append({'path':path,'kind':'length','baseline':len(left),'candidate':len(right)})
            for index,(a,b) in enumerate(zip(left,right)):visit(a,b,path+[index])
        elif left!=right:changes.append({'path':path,'kind':'value','baseline':left,'candidate':right})
    visit(baseline,candidate,[])
    return {'exact':not changes,'changed_leaves':len(changes),'differences':changes}

CHECKED_RANGE_PATHS = (
    ('checked', 'occurrences', int, 'source', 'range', 'end'),
    ('checked', 'scopes', int, 'source', 'range', 'end'),
    ('checked', 'declarations', int, 'noreturn_source', 'range', 'end'),
)

def _checked_range_path(path):
    return any(len(path) == len(pattern) and all(
        type(part) is int if expected is int else part == expected
        for part, expected in zip(path, pattern)
    ) for pattern in CHECKED_RANGE_PATHS)

def _follow(value, path):
    for key in path:
        value = value[key]
    return value

def compare_checked(baseline, candidate, workload=None, corrections=None):
    """Admit audited whitespace trims and exact, source-pinned reviewed corrections."""
    if baseline.get('source') != candidate.get('source') or not isinstance(baseline.get('source'), str):
        return {'accepted': False, 'kind': 'checked_source_differs'}
    source = baseline['source'].encode('utf-8')
    diagnosis = diagnose_json(baseline, candidate)
    changes = []
    restored_delimiters = []
    source_sha256 = hashlib.sha256(source).hexdigest()
    reviewed = (corrections or {}).get('corrections', [])
    if corrections is not None and (corrections.get('version') != 1 or corrections.get('reviewed') is not True or corrections.get('count') != len(reviewed)):
        return {'accepted': False, 'kind': 'invalid_correction_manifest'}
    for difference in diagnosis['differences']:
        path = difference['path']
        if difference['kind'] != 'value' or not _checked_range_path(path):
            return {'accepted': False, 'kind': 'unreviewed_checked_field_changed', 'difference': difference}
        old_source = _follow(baseline, path[:-2])
        new_source = _follow(candidate, path[:-2])
        if any(value.get('synthetic') is not False or value.get('fragments') != [] for value in (old_source, new_source)):
            return {'accepted': False, 'kind': 'checked_source_not_contiguous', 'path': path}
        old, new = old_source['range'], new_source['range']
        if any(set(value) != {'start', 'end'} or any(type(offset) is not int for offset in value.values()) for value in (old, new)):
            return {'accepted': False, 'kind': 'invalid_checked_range', 'path': path}
        start, end, new_start, new_end = old['start'], old['end'], new['start'], new['end']
        if not (start == new_start and 0 <= start <= new_end < end <= len(source)):
            occurrence = _follow(candidate, path[:-3])
            matches = [item for item in reviewed if (
                item['workload'] == workload and item['source_sha256'] == source_sha256
                and item['path'] == path and item['baseline'] == old and item['candidate'] == new
                and item['occurrence_kind'] == occurrence.get('kind')
                and item['restored_bytes_hex'] == '29'
            )]
            if (len(matches) == 1 and path[:2] == ['checked', 'occurrences']
                    and start == new_start and 0 <= start <= end < new_end <= len(source)
                    and new_end == end + 1 and source[end:new_end] == b')'
                    and source[start:new_end].decode('utf-8') == matches[0]['candidate_spelling']):
                restored_delimiters.append(matches[0])
                continue
            return {'accepted': False, 'kind': 'checked_change_is_not_a_suffix_trim', 'path': path, 'old': old, 'new': new}
        suffix = source[new_end:end]
        if not suffix.isspace():
            return {'accepted': False, 'kind': 'checked_span_removed_non_whitespace', 'path': path, 'old': old, 'new': new, 'removed_bytes_hex': suffix.hex()}
        changes.append({'path': path, 'old': old, 'new': new, 'removed_bytes_hex': suffix.hex()})
    return {'accepted': True, 'kind': 'reviewed_checked_spans' if changes or restored_delimiters else 'exact_checked', 'exact_spans': not changes and not restored_delimiters, 'changed_span_count': len(changes) + len(restored_delimiters), 'whitespace_trim_count': len(changes), 'restored_delimiter_count': len(restored_delimiters), 'changes': changes, 'restored_delimiters': restored_delimiters}
