import yaml, sys
files = sys.argv[1:]
errors = []
for f in files:
    try:
        data = yaml.safe_load(open(f))
        assert data.get('schema_version') == 1, 'missing schema_version: 1'
        assert data.get('targets'), 'no targets'
        for t in data['targets']:
            assert t.get('id'), 'target missing id'
    except Exception as e:
        errors.append(f'{f}: {e}')
if errors:
    [print(e, file=sys.stderr) for e in errors]
    sys.exit(1)
print(f'OK: {len(files)} file(s) valid')
