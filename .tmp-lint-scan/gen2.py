import json, copy

p = '.rust-migration/migration-state.json'
s = json.load(open(p))
tpl = list(s['modules'].values())[0]


def mk(status, members=None, blocked_by=None, pre=None):
    m = copy.deepcopy(tpl)
    m['status'] = status
    m.pop('member_files', None)
    m['composite_kind'] = None
    m['blocked_by'] = blocked_by
    m['pre_blocked_status'] = pre
    m['substatus'] = None
    m['test_pass_rate'] = '0.9'
    m['coverage'] = 80
    if members:
        m['member_files'] = members
        m['composite_kind'] = 'coupled_batch'
    return m


s['modules'] = {
    'g1': mk('translating', ['g1', 'file:a.ts']),
    'g2': mk('pending', ['g2', 'g1']),
}
json.dump(s, open(p, 'w'), ensure_ascii=False, indent=1)
print('written')
