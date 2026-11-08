import { describe, expect, it } from 'vitest';

import { importGroup, sortImports } from './imports';

const sorted = (text: string): string => {
  const e = sortImports(text);
  return e ? text.slice(0, e.from) + e.insert + text.slice(e.to) : text;
};

describe('importGroup', () => {
  it('puts std first, a dependency second and this project last', () => {
    expect(importGroup('std::parts::Resistor')).toBe(0);
    expect(importGroup('acme::parts::Chip')).toBe(1);
    expect(importGroup('src::sheets::Power')).toBe(2);
  });
});

describe('sortImports', () => {
  it("sorts the imports of a board with a project sheet, std parts and a std process into std, then src, with a blank line between", () => {
    const text = `import src::sheets::input_protection::InputProtection
import std::mfg::jlcpcb::FourLayer
import std::parts::connector::Obd2M

import std::parts::connector::Header1x2

board Guard {
}
`;
    expect(sorted(text)).toBe(`import std::mfg::jlcpcb::FourLayer
import std::parts::connector::Header1x2
import std::parts::connector::Obd2M

import src::sheets::input_protection::InputProtection

board Guard {
}
`);
  });

  it('puts a dependency between the two', () => {
    expect(sorted('import src::a::A\nimport acme::b::B\nimport std::c::C\n\nboard X {}\n')).toBe(
      'import std::c::C\n\nimport acme::b::B\n\nimport src::a::A\n\nboard X {}\n',
    );
  });

  it('leaves module docs and what follows the imports alone', () => {
    const text = `//! The input protection.
import src::parts::b::B
import std::sources::Rail

/// docs
module M {}
`;
    expect(sorted(text)).toBe(`//! The input protection.
import std::sources::Rail

import src::parts::b::B

/// docs
module M {}
`);
  });

  it('takes the comment above an import with it', () => {
    expect(sorted('import src::z::Z\n// the rail\nimport std::sources::Rail\n')).toBe(
      '// the rail\nimport std::sources::Rail\n\nimport src::z::Z\n',
    );
  });

  it('keeps a braced list, over several lines too', () => {
    expect(sorted('import src::z::Z\nimport std::parts::passive::{\n    Resistor,\n    Led,\n}\n')).toBe(
      'import std::parts::passive::{\n    Resistor,\n    Led,\n}\n\nimport src::z::Z\n',
    );
  });

  it('is already sorted: no change, and sorting twice is the same as once', () => {
    const text = 'import std::a::A\nimport std::b::B\n\nimport src::c::C\n\nboard X {}\n';
    expect(sortImports(text)).toBeNull();
    const messy = 'import src::c::C\n\n\nimport std::b::B\nimport std::a::A\nboard X {}\n';
    expect(sorted(sorted(messy))).toBe(sorted(messy));
  });

  it('has nothing to do in a file with no imports', () => {
    expect(sortImports('board X {}\n')).toBeNull();
    expect(sortImports('')).toBeNull();
  });

  it('keeps the line endings of the file', () => {
    expect(sorted('import src::c::C\r\nimport std::a::A\r\n')).toBe('import std::a::A\r\n\r\nimport src::c::C\r\n');
  });

  it('keeps a comment that ends a line', () => {
    expect(sorted('import src::c::C // mine\nimport std::a::A\n')).toBe('import std::a::A\n\nimport src::c::C // mine\n');
  });
});

describe('sortImports and loose comments', () => {
  it('keeps a comment set apart by a blank line where it is', () => {
    const text = '// a note\n\nimport src::c::C\nimport std::a::A\n';
    expect(sorted(text)).toBe('// a note\n\nimport std::a::A\n\nimport src::c::C\n');
  });

  it('stops at a comment set apart after the imports, and loses nothing', () => {
    const text = 'import src::c::C\nimport std::a::A\n// later\n\nimport std::b::B\nboard X {}\n';
    const out = sorted(text);
    expect(out).toContain('// later');
    expect(out).toContain('import std::b::B');
    expect(out.split('\n').filter((l) => l.startsWith('import')).length).toBe(3);
  });
});
