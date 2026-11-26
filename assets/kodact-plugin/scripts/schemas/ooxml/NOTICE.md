# Office Open XML schemas

The `.xsd` files here, except `xml.xsd`, are the W3C XML Schema files of ECMA-376 Part 4, 5th
edition (December 2016), "Office Open XML File Formats — Transitional Migration Features"
(`OfficeOpenXML-XMLSchema-Transitional.zip`), © Ecma International, published at
https://ecma-international.org/publications-and-standards/standards/ecma-376/.

Ecma International publishes its standards free of charge and allows them to be copied and
distributed, and derivative works that assist in implementing them to be prepared and
distributed, provided its copyright notice is kept. KodAct uses them unmodified, except:

- the two `xsd:import`s of the XML namespace (`wml.xsd`, `shared-math.xsd`) name `xml.xsd`, a small
  file written for KodAct that declares `xml:space`, `xml:lang`, `xml:base` and `xml:id`;
- in `dml-main.xsd`, `CT_TextBulletSizePercent/@val` is `ST_TextBulletSize` (the union of the
  percentage text "45%" and the number 45000) instead of its text half alone: PowerPoint writes
  the number, and the schema's own union type exists for this attribute.

Where Office itself departs from these schemas in other ways, `../ooxml_schema.py` reads the
file as Office does (its `tolerate`), each case with its reason.

They are used by `../ooxml_schema.py` to check the files the document skills write.
