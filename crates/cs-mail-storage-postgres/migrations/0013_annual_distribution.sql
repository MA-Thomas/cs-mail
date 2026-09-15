ALTER TABLE cs_program_quarters RENAME TO cs_program_annual_allocations;
ALTER TABLE cs_financial_programs DROP CONSTRAINT cs_financial_programs_program_format_version_check;
ALTER TABLE cs_financial_programs ADD CHECK(program_format_version IN(1,2,3,4,5,6));
INSERT INTO cs_schema_migrations(version) VALUES(13);
