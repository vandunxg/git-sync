"""Tests for blueprint utilities only; not Git Backup Manager acceptance tests."""
from __future__ import annotations
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import sys
import tempfile
import unittest
import zlib

SCRIPTS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(SCRIPTS))
from contract_checks import validate,schema_errors
from check_visual_baselines import audit,contained_file,png_dimensions
from lint_pack import lint,acyclic
from run_gates import preflight,redact,run_command
from check_task_scope import allowed,check_paths
ROOT=SCRIPTS.parent


def write_json(path: Path, data: object) -> None:
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(data),encoding='utf-8')


def png(width: int=2,height: int=2) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    body=(b'\0'+b'\xff\xff\xff'*width)*height
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(body))+chunk(b'IEND',b'')


class ContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root={'$defs':{'Id':{'type':'string','pattern':'^r-[0-9]+$'}}}
    def test_valid_local_reference(self):
        self.assertEqual(validate('r-23',{'$ref':'#/$defs/Id'},self.root),[])
    def test_invalid_local_reference(self):
        self.assertTrue(validate('x',{'$ref':'#/$defs/Absent'},self.root))
    def test_bool_is_not_integer(self):
        self.assertTrue(validate(True,{'type':'integer'},self.root))
    def test_nullable_type(self):
        s={'type':['string','null']}
        self.assertFalse(validate(None,s,self.root));self.assertTrue(validate(2,s,self.root))
    def test_strict_object(self):
        s={'type':'object','required':['id'],'properties':{'id':{'type':'string'}},'additionalProperties':False}
        self.assertFalse(validate({'id':'x'},s,self.root))
        self.assertTrue(validate({'id':'x','token':'bad'},s,self.root))
        self.assertTrue(validate({},s,self.root))
    def test_array_and_range(self):
        s={'type':'array','maxItems':2,'items':{'type':'integer','minimum':0,'maximum':10}}
        self.assertFalse(validate([0,10],s,self.root))
        self.assertTrue(validate([11],s,self.root));self.assertTrue(validate([1,2,3],s,self.root))
    def test_anyof(self):
        s={'anyOf':[{'type':'null'},{'type':'integer','minimum':1}]}
        self.assertFalse(validate(None,s,self.root));self.assertTrue(validate(0,s,self.root))
    def test_timezone_required(self):
        s={'type':'string','format':'date-time'}
        self.assertFalse(validate('2026-10-01T13:20:00Z',s,self.root))
        self.assertTrue(validate('2026-10-01T13:20:00',s,self.root))
    def test_schema_subset_rejects_unknown_keyword(self):
        self.assertTrue(schema_errors({'type':'string','fictionalConstraint':True}))
    def test_fixture_roundtrip(self):
        schema=json.loads((ROOT/'contracts/dtos.schema.json').read_text())
        fixtures=json.loads((ROOT/'fixtures/ui-seed.json').read_text())['dtoFixtures']
        self.assertFalse(schema_errors(schema))
        for f in fixtures:
            self.assertEqual(validate(f['value'],schema['$defs'][f['type']],schema),[])


class BaselineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
        write_json(self.root/'docs/ui/SCREENS.json',{'screens':[{'id':'UI-TEST','baselineRequired':True}]})
        self.entry={'screenId':'UI-TEST','status':'missing','viewport':{'width':2,'height':2},
                    'designReference':{'path':None,'sha256':None},'implementationGolden':{},'approval':{}}
        self.save()
    def tearDown(self): self.tmp.cleanup()
    def save(self): write_json(self.root/'docs/ui/BASELINES.json',{'screens':[self.entry]})
    def approved_fixture(self):
        image=png(); (self.root/'design.png').write_bytes(image)
        (self.root/'review.md').write_text('TEST FIXTURE, not a real owner approval.')
        self.entry.update(status='approved',designReference={'path':'design.png','sha256':hashlib.sha256(image).hexdigest()},
                          approval={'by':'test-human','at':'2026-10-01T13:20:00Z','recordPath':'review.md'})
        self.save()
    def test_missing_approval_blocks(self): self.assertEqual(audit(self.root)['status'],'BLOCKED')
    def test_integrity_pass_is_not_visual_approval(self):
        self.approved_fixture();result=audit(self.root)
        self.assertEqual(result['status'],'PASS_RECORD_INTEGRITY_ONLY');self.assertFalse(result['visualApprovalProven'])
    def test_hash_mismatch_blocks(self):
        self.approved_fixture();self.entry['designReference']['sha256']='0'*64;self.save()
        self.assertEqual(audit(self.root)['status'],'BLOCKED')
    def test_path_traversal_blocks(self):
        with self.assertRaises(ValueError): contained_file(self.root,'../outside.png')
    def test_dimensions_mismatch_blocks(self):
        self.approved_fixture();self.entry['viewport']['width']=3;self.save()
        self.assertEqual(audit(self.root)['status'],'BLOCKED')
    def test_same_design_and_implementation_not_allowed(self):
        self.approved_fixture();self.entry['implementationGolden']={**self.entry['designReference'],'commit':'test-only','renderer':'test-renderer'};self.save()
        self.assertEqual(audit(self.root,True)['status'],'BLOCKED')
    def test_invalid_png_rejected(self):
        p=self.root/'invalid.png';p.write_bytes(b'not an image')
        with self.assertRaises(ValueError): png_dimensions(p)


class GateAndRegistryTests(unittest.TestCase):
    def test_dependency_cycle(self):
        self.assertTrue(acyclic({'a':[],'b':['a']}))
        self.assertFalse(acyclic({'a':['b'],'b':['a']}))
        self.assertFalse(acyclic({'a':['missing']}))
    def test_pack_lints(self):
        result=lint(ROOT);self.assertEqual(result['errors'],[],result)
    def test_broken_registry_detected(self):
        with tempfile.TemporaryDirectory() as temp:
            target=Path(temp)/'copy'
            shutil.copytree(ROOT,target,ignore=shutil.ignore_patterns('__pycache__','local'))
            path=target/'docs/plan/TASKS.json';data=json.loads(path.read_text());data['tasks'][0]['requirements'].append('REQ-NONEXISTENT');write_json(path,data)
            self.assertEqual(lint(target)['status'],'FAIL')
    def test_missing_app_blocks_before_execution(self):
        with tempfile.TemporaryDirectory() as temp:
            reasons=preflight({'requireFiles':['Cargo.toml'],'commands':[{'argv':[sys.executable,'-c','print(1)']}]},Path(temp))
            self.assertTrue(any('Cargo.toml' in r for r in reasons))
    def test_missing_package_script_blocks(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);write_json(root/'apps/desktop/package.json',{'scripts':{}})
            self.assertTrue(preflight({'packageScripts':['test:unit'],'commands':[]},root))
    def test_native_requires_authorization(self):
        self.assertTrue(preflight({'requiresNativeAuthorization':True,'commands':[]},ROOT))
    def test_redaction(self):
        synthetic='Authorization: Bearer fake-value\nghp_TEST_ONLY_MARKER\nglpat-TEST_ONLY_MARKER\nhttps://user:fake-password@example.test/r\naccess_token=fake'
        clean=redact(synthetic)
        self.assertNotIn('TEST_ONLY',clean);self.assertNotIn('fake-password',clean);self.assertNotIn('fake-value',clean)
    def test_command_success(self):
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);result=run_command([sys.executable,'-c','print("hello")'],p,p/'out.log',5)
            self.assertEqual(result['status'],'PASS');self.assertEqual(result['exitCode'],0)
    def test_command_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);result=run_command([sys.executable,'-c','raise SystemExit(7)'],p,p/'out.log',5)
            self.assertEqual(result['status'],'FAIL');self.assertEqual(result['exitCode'],7)
    def test_command_missing_is_blocked(self):
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);result=run_command([str(p/'no-such-program')],p,p/'out.log',5)
            self.assertEqual(result['status'],'BLOCKED')
    def test_timeout_is_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);result=run_command([sys.executable,'-c','import time; time.sleep(20)'],p,p/'out.log',0.15)
            self.assertEqual(result['status'],'FAIL');self.assertTrue(result['timedOut'])
    def test_output_cap_and_redaction(self):
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);code='print("ghp_TEST_ONLY_MARKER"); print("normal line\\n"*2000)'
            result=run_command([sys.executable,'-c',code],p,p/'out.log',5,max_log_bytes=512)
            out=(p/'out.log').read_text()
            self.assertEqual(result['status'],'PASS');self.assertTrue(result['logCapped'])
            self.assertNotIn('TEST_ONLY_MARKER',out);self.assertLess(len(out),650)


class ScopeTests(unittest.TestCase):
    def test_exact_file(self):
        self.assertTrue(allowed('Cargo.toml',['Cargo.toml']))
        self.assertFalse(allowed('Cargo.toml.bak',['Cargo.toml']))
    def test_directory_boundary(self):
        self.assertTrue(allowed('crates/domain/src/lib.rs',['crates/domain/']))
        self.assertFalse(allowed('crates/domain-evil/lib.rs',['crates/domain/']))
    def test_traversal_denied(self):
        self.assertFalse(allowed('../state/evidence/fake.md',['state/']))
    def test_nested_blueprint_and_common_evidence(self):
        t={'allowedPaths':['apps/desktop/src/features/dashboard/']}
        paths=['apps/desktop/src/features/dashboard/Page.tsx','blueprint/state/evidence/report.md','secrets.txt']
        self.assertEqual(check_paths(paths,t,'blueprint'),['secrets.txt'])

if __name__=='__main__': unittest.main()
