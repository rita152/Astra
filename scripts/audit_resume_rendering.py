#!/usr/bin/env python3
"""Read-only per-record audit against a captured real ChatGPT DOM and thread/read.

The JSONL is a diagnostic input only; the application's resume path still uses
app-server. No prompts, tool invocations, or source-file mutations are performed.
"""
import argparse
from collections import Counter
import json
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--jsonl', type=Path, required=True)
    p.add_argument('--history', type=Path, required=True)
    p.add_argument('--dom', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--native', type=Path, help='GPUI screenshot .render.json companion')
    a = p.parse_args()
    records = [json.loads(s) for s in a.jsonl.read_text().splitlines()]
    thread = json.loads(a.history.read_text())['result']['thread']
    dom = json.loads(a.dom.read_text())['value']
    items = {i['id']: i for t in thread['turns'] for i in t['items']}
    targets = {}
    for n, group in enumerate(dom['groups']):
        for i in group['ids']:
            targets[i.split(':')[0]] = {'group': n, 'title': group['title']}
    for message in dom['messages']:
        attrs = message['attrs']
        i = attrs.get('data-response-annotation-target')
        if not i and attrs.get('data-local-conversation-user-anchor'):
            i = attrs['data-content-search-unit-key'].split(':', 1)[-1]
        if i:
            targets[i] = {'message': True, 'text': message['text'][:160]}
    # Adjacent view_image calls share one disclosure in the actual DOM.
    for turn in thread['turns']:
        previous = None
        for item in turn['items']:
            if item['type'] == 'imageView':
                if item['id'] not in targets and previous in targets:
                    targets[item['id']] = dict(targets[previous], merged_image=True)
                previous = item['id']
            elif item['type'] != 'reasoning':
                previous = None
    # These special renderer components do not expose an item target id.
    # Match their actual visible text rather than inventing a DOM identity.
    for item in items.values():
        if item['type'] == 'webSearch':
            for n, group in enumerate(dom['groups']):
                if ('已搜索网页 ：' + item['query']) in group.get('text', ''):
                    targets[item['id']] = {'group': n, 'title': group['title'], 'matched_by': 'query_text'}
        elif item['type'] == 'userMessage':
            text = '\n'.join(c.get('text', '') for c in item['content'])
            if text.startswith('<send_user_message_question_reply>'):
                replies = json.loads(text.split('>', 1)[1].rsplit('<', 1)[0])
                if all(any(r['answer'] == m['text'] for m in dom['messages']) for r in replies):
                    targets[item['id']] = {'question_reply': True, 'matched_by': 'answer_text'}
    calls = {}
    active_exec = None
    for n, record in enumerate(records, 1):
        q = record.get('payload', {})
        if record['type'] == 'response_item' and q.get('type') in ('custom_tool_call', 'function_call'):
            calls[q['call_id']] = {'line': n, 'name': q['name'], 'items': []}
            if q['type'] == 'custom_tool_call':
                active_exec = q['call_id']
        if q.get('type') == 'item_completed' and active_exec:
            calls[active_exec]['items'].append(q['item']['id'])
        if q.get('type') == 'custom_tool_call_output':
            active_exec = None
    rows = []
    for n, record in enumerate(records, 1):
        q = record.get('payload', {})
        kind = q.get('type', '')
        row = {'line': n, 'type': record['type'], 'payload_type': kind, 'timestamp': record.get('timestamp')}
        if kind == 'item_completed':
            item = q['item']; i = item['id']
            row.update(item_id=i, item_type=item['type'], app_server_type=items.get(i, {}).get('type'), started_at_ms=q.get('started_at_ms'), completed_at_ms=q.get('completed_at_ms'))
            if i in targets:
                row.update(disposition='rendered', chatgpt=targets[i])
            elif item['type'] == 'Reasoning':
                row['disposition'] = 'reasoning_context_no_independent_row'
            else:
                row['disposition'] = 'unmatched_item_requires_review'
            if item['type'] == 'McpToolCall':
                row.update(tool=item['tool'], title=item['arguments'].get('title'), surface=item.get('result', {}).get('_meta', {}).get('codex/toolSurface'))
        elif record['type'] == 'response_item' and kind in ('custom_tool_call', 'function_call', 'custom_tool_call_output', 'function_call_output'):
            call = calls.get(q.get('call_id'), {})
            row.update(disposition='tool_transport_no_extra_row', call_id=q.get('call_id'), tool=call.get('name'), item_ids=call.get('items', []) or ([q['call_id']] if q.get('call_id') in items else []))
        elif record['type'] == 'response_item':
            row['disposition'] = 'model_record_no_duplicate_row'
            if q.get('id') in items:
                row['item_id'] = q['id']
        else:
            row['disposition'] = 'session_or_turn_metadata_no_row'
        rows.append(row)
    native_targets = {}
    group_comparisons = []
    if a.native:
        native = json.loads(a.native.read_text())
        for turn in native['turns']:
            source_turn = next(t for t in thread['turns'] if t['id'] == turn['id'])
            first_user = next((i for i in source_turn['items'] if i['type'] == 'userMessage'), None)
            if first_user:
                native_targets[first_user['id']] = {'type': 'user', 'text': turn['user_message']}
            for unit in turn['units']:
                if unit['type'] == 'toolGroup':
                    ids = {i['id'] for i in unit['items'] if 'id' in i}
                    groups = sorted({targets[i]['group'] for i in ids if i in targets and 'group' in targets[i]})
                    reference_ids = {i for i, target in targets.items() if target.get('group') in groups}
                    group_comparisons.append({'turn_id': turn['id'], 'label': unit['label'],
                        'gpui_item_ids': sorted(ids), 'chatgpt_groups': groups,
                        'same_membership': len(groups) == 1 and ids == reference_ids,
                        'reference_extra_ids': sorted(reference_ids - ids)})
                for item in unit.get('items', [unit]):
                    target = dict(item, group=unit.get('label'))
                    for i in ([item['id']] if 'id' in item else item.get('ids', [])):
                        native_targets[i] = target
        for row in rows:
            if row.get('item_id') in native_targets:
                row['gpui'] = native_targets[row['item_id']]
    native_missing = [r['item_id'] for r in rows if r['disposition'] == 'rendered' and r.get('item_id') not in native_targets] if a.native else None
    report = {'source': str(a.jsonl), 'thread_id': thread['id'], 'record_count': len(records), 'app_server_items': len(items),
              'counts': dict(Counter(r['disposition'] for r in rows)), 'native_missing_rendered_items': native_missing,
              'unmatched_items': [r for r in rows if r['disposition'] == 'unmatched_item_requires_review'],
              'tool_group_comparisons': group_comparisons,
              'records': rows}
    a.output.write_text(json.dumps(report, ensure_ascii=False, indent=2))
    print(json.dumps({k:v for k,v in report.items() if k not in ('records', 'tool_group_comparisons')}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
