#!/usr/bin/env python3
"""Small, strict validator for the schema subset used by this blueprint.

Not a general JSON Schema implementation. External references are unsupported.
"""
from __future__ import annotations
import math
import re
from datetime import datetime
from typing import Any

SUPPORTED = {'$schema', '$comment', '$defs', 'title', 'description', '$ref', 'type',
             'anyOf', 'properties', 'required', 'additionalProperties', 'items',
             'minItems', 'maxItems', 'minLength', 'maxLength', 'pattern', 'format',
             'minimum', 'maximum', 'enum', 'const', 'writeOnly', 'readOnly'}


def schema_errors(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    defs = schema.get('$defs', {})
    def walk(node: Any, path: str) -> None:
        if not isinstance(node, dict):
            errors.append(f'{path}: expected schema object'); return
        for key in node:
            if key not in SUPPORTED:
                errors.append(f'{path}: unsupported schema keyword {key}')
        ref = node.get('$ref')
        if ref and (not ref.startswith('#/$defs/') or ref[8:] not in defs):
            errors.append(f'{path}: unresolved local reference {ref}')
        if 'pattern' in node:
            try: re.compile(node['pattern'])
            except re.error as exc: errors.append(f'{path}: invalid regex: {exc}')
        if node.get('format') not in (None, 'date-time'):
            errors.append(f'{path}: unsupported format')
        for key in ('$defs', 'properties'):
            for name, value in node.get(key, {}).items(): walk(value, f'{path}/{key}/{name}')
        for i, branch in enumerate(node.get('anyOf', [])): walk(branch, f'{path}/anyOf/{i}')
        if 'items' in node: walk(node['items'], f'{path}/items')
    walk(schema, '#')
    return errors


def validate(value: Any, schema: dict[str, Any], root: dict[str, Any], path: str = '$', depth: int = 0) -> list[str]:
    if depth > 80: return [f'{path}: schema recursion limit']
    errors: list[str] = []
    if '$ref' in schema:
        ref = schema['$ref']
        target = root.get('$defs', {}).get(ref[8:]) if ref.startswith('#/$defs/') else None
        if target is None: return [f'{path}: unresolved reference {ref}']
        errors += validate(value, target, root, path, depth + 1)
    if 'anyOf' in schema:
        branches = [validate(value, s, root, path, depth + 1) for s in schema['anyOf']]
        if not any(not b for b in branches): errors.append(f'{path}: no anyOf branch matched')
    expected = schema.get('type')
    checks = {
        'object': lambda: isinstance(value, dict),
        'array': lambda: isinstance(value, list),
        'string': lambda: isinstance(value, str),
        'integer': lambda: type(value) is int,
        'number': lambda: type(value) in (int, float) and math.isfinite(value),
        'boolean': lambda: type(value) is bool,
        'null': lambda: value is None,
    }
    expected_types = expected if isinstance(expected, list) else [expected]
    if expected and not any(t in checks and checks[t]() for t in expected_types):
        return errors + [f'{path}: expected {expected}']
    def equal(a: Any, b: Any) -> bool:
        if isinstance(a, bool) or isinstance(b, bool): return type(a) is type(b) and a == b
        return a == b
    if 'const' in schema and not equal(value, schema['const']): errors.append(f'{path}: const mismatch')
    if 'enum' in schema and not any(equal(value, v) for v in schema['enum']): errors.append(f'{path}: enum mismatch')
    if isinstance(value, dict):
        props = schema.get('properties', {})
        for key in schema.get('required', []):
            if key not in value: errors.append(f'{path}.{key}: required')
        if schema.get('additionalProperties') is False:
            errors += [f'{path}.{key}: unexpected property' for key in value if key not in props]
        for key, item in value.items():
            if key in props: errors += validate(item, props[key], root, f'{path}.{key}', depth + 1)
    if isinstance(value, list):
        if 'minItems' in schema and len(value) < schema['minItems']: errors.append(f'{path}: too few items')
        if 'maxItems' in schema and len(value) > schema['maxItems']: errors.append(f'{path}: too many items')
        if 'items' in schema:
            for i, item in enumerate(value): errors += validate(item, schema['items'], root, f'{path}[{i}]', depth + 1)
    if isinstance(value, str):
        if 'minLength' in schema and len(value) < schema['minLength']: errors.append(f'{path}: too short')
        if 'maxLength' in schema and len(value) > schema['maxLength']: errors.append(f'{path}: too long')
        if 'pattern' in schema and not re.search(schema['pattern'], value): errors.append(f'{path}: pattern mismatch')
        if schema.get('format') == 'date-time':
            try:
                dt = datetime.fromisoformat(value.replace('Z', '+00:00'))
                if dt.tzinfo is None: raise ValueError('timezone missing')
            except (ValueError, TypeError): errors.append(f'{path}: invalid timezone-qualified date-time')
    if type(value) in (int, float):
        if 'minimum' in schema and value < schema['minimum']: errors.append(f'{path}: below minimum')
        if 'maximum' in schema and value > schema['maximum']: errors.append(f'{path}: above maximum')
    return errors
