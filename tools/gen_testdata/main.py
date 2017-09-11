import os
import datetime

from pyamf import amf0, amf3, AMF0, AMF3, MixedArray, xml, register_class, TypedObject
from pyamf.amf0 import Encoder as AMF0Encoder
from pyamf.amf3 import Encoder as AMF3Encoder, MIN_29B_INT, MAX_29B_INT, \
    ASDictionary, IntVector, UintVector, \
    DoubleVector, ObjectVector, ByteArray
import simplejson

##############################
#           COMMON           #
##############################

OUTPUT_DIR = os.path.join(os.path.dirname(
    os.path.realpath(__file__)), "../../testdata")

NUMBER_NEGATIVE_INFINITY = "-Infinity"
NUMBER_POSITIVE_INFINITY = "Infinity"
NUMBER_NAN = "NaN"


class HogeClass:
    def __init__(self, index, msg):
        self.index = index
        self.msg = msg


class NoDynamicHogeClass:
    def __init__(self, index, msg):
        self.index = index
        self.msg = msg

    class __amf__:
        dynamic = False


class DynamicHogeClass:
    def __init__(self, index, msg):
        self.index = index
        self.msg = msg

    class __amf__:
        dynamic = True


register_class(HogeClass, "com.pyyoshi.hogeclass")
register_class(DynamicHogeClass, "com.pyyoshi.dynamichogeclass")
register_class(NoDynamicHogeClass, "com.pyyoshi.nodynamichogeclass")


def build_result(name, amf_version, amf_type, amf_value, amf_bytes, classname=None):
    return {
        "name": name,
        "version": amf_version,
        "type": int.from_bytes(amf_type, byteorder='big'),
        "classname": classname,
        "value": amf_value,
        "amf_bytes": amf_bytes,
    }


def write_json(output_dir, result):
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)

    output_json_path = os.path.join(output_dir, result["name"] + ".json")
    with open(output_json_path, "w") as fp_json:
        amf_file_name = result["name"] + ".bin"
        output_amf_bin_path = os.path.join(output_dir, amf_file_name)
        with open(output_amf_bin_path, "wb") as fp_amf_bin:
            fp_amf_bin.write(result["amf_bytes"])

            del result["name"]
            del result["amf_bytes"]
            result["file"] = amf_file_name
            simplejson.dump(result, fp_json, sort_keys=True,
                            indent="    ", ensure_ascii=False)


##############################
#            AMF0            #
##############################


def gen_amf0_number():
    v1 = 1234.5
    enc1 = AMF0Encoder()
    enc1.writeNumber(v1)
    result1 = build_result(
        "amf0-number",
        AMF0,
        amf0.TYPE_NUMBER,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    enc2 = AMF0Encoder()
    enc2.writeType(amf0.TYPE_NUMBER)
    enc2.stream.write(b"\xff\xf0\x00\x00\x00\x00\x00\x00")
    result2 = build_result(
        "amf0-number-negative-infinity",
        AMF0,
        amf0.TYPE_NUMBER,
        NUMBER_NEGATIVE_INFINITY,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)

    enc3 = AMF0Encoder()
    enc3.writeType(amf0.TYPE_NUMBER)
    enc3.stream.write(b"\x7f\xf0\x00\x00\x00\x00\x00\x00")
    result3 = build_result(
        "amf0-number-positive-infinity",
        AMF0,
        amf0.TYPE_NUMBER,
        NUMBER_POSITIVE_INFINITY,
        enc3.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result3)

    enc4 = AMF0Encoder()
    enc4.writeType(amf0.TYPE_NUMBER)
    enc4.stream.write(b"\xff\xf8\x00\x00\x00\x00\x00\x00")
    result4 = build_result(
        "amf0-number-nan",
        AMF0,
        amf0.TYPE_NUMBER,
        NUMBER_NAN,
        enc4.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result4)


def gen_amf0_boolean():
    v1 = True
    enc1 = AMF0Encoder()
    enc1.writeBoolean(v1)
    result1 = build_result(
        "amf0-boolean-true",
        AMF0,
        amf0.TYPE_BOOL,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = False
    enc2 = AMF0Encoder()
    enc2.writeBoolean(v2)
    result2 = build_result(
        "amf0-boolean-false",
        AMF0,
        amf0.TYPE_BOOL,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)


def gen_amf0_string():
    v1 = "Hello, world!"
    enc1 = AMF0Encoder()
    enc1.writeString(v1)
    result1 = build_result(
        "amf0-string",
        AMF0,
        amf0.TYPE_STRING,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_object():
    v1 = {
        "msg": "Hello, world! こんにちは、世界！",
        "index": 0,
    }
    enc1 = AMF0Encoder()
    enc1.writeObject(v1)
    result1 = build_result(
        "amf0-object",
        AMF0,
        amf0.TYPE_OBJECT,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_movieclip():
    enc1 = AMF0Encoder()
    enc1.writeType(amf0.TYPE_MOVIECLIP)
    result1 = build_result(
        "amf0-movieclip",
        AMF0,
        amf0.TYPE_MOVIECLIP,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_null():
    enc1 = AMF0Encoder()
    enc1.writeNull("")
    result1 = build_result(
        "amf0-null",
        AMF0,
        amf0.TYPE_NULL,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_undefined():
    enc1 = AMF0Encoder()
    enc1.writeUndefined("")
    result1 = build_result(
        "amf0-undefined",
        AMF0,
        amf0.TYPE_UNDEFINED,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_reference():
    v1 = [1, 2, 3]
    enc1 = AMF0Encoder()
    enc1.writeList(v1)
    enc1.writeList(v1)
    result1 = build_result(
        "amf0-reference-array-number",
        AMF0,
        amf0.TYPE_REFERENCE,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = ["foo", "baz", "bar"]
    enc2 = AMF0Encoder()
    enc2.writeList(v2)
    enc2.writeList(v2)
    result2 = build_result(
        "amf0-reference-array-string",
        AMF0,
        amf0.TYPE_REFERENCE,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)

    v3 = {
        "msg": "Hello, world! こんにちは、世界！",
        "index": 0,
    }
    enc3 = AMF0Encoder()
    enc3.writeObject(v3)
    enc3.writeObject(v3)
    result3 = build_result(
        "amf0-reference-object",
        AMF0,
        amf0.TYPE_REFERENCE,
        v3,
        enc3.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result3)


def gen_amf0_ecma_array():
    v1 = MixedArray(en="Hello, world!", ja="こんにちは、世界！", zh="你好世界")
    enc1 = AMF0Encoder()
    enc1.writeMixedArray(v1)
    result1 = build_result(
        "amf0-ecma-array",
        AMF0,
        amf0.TYPE_MIXEDARRAY,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_object_end():
    enc1 = AMF0Encoder()
    enc1.writeType(amf0.TYPE_OBJECTTERM)
    result1 = build_result(
        "amf0-object-end",
        AMF0,
        amf0.TYPE_OBJECTTERM,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_strict_array():
    v1 = [1.1, 2, 3.3, "こんにちは、世界！"]
    enc1 = AMF0Encoder()
    enc1.writeList(v1)
    result1 = build_result(
        "amf0-strict-array",
        AMF0,
        amf0.TYPE_ARRAY,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_date():
    v1 = datetime.datetime(2005, 3, 18, 1, 58, 31)
    enc1 = AMF0Encoder()
    enc1.writeDate(v1)
    result1 = build_result(
        "amf0-date",
        AMF0,
        amf0.TYPE_DATE,
        v1.timestamp(),
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_longstring():
    v1 = "うひょおおおおおおおおおおおおおおおおおおおおおおおおおおおおおお" * 2000
    enc1 = AMF0Encoder()
    enc1.writeString(v1)
    result1 = build_result(
        "amf0-long-string",
        AMF0,
        amf0.TYPE_LONGSTRING,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_unsupported():
    enc1 = AMF0Encoder()
    enc1.writeType(amf0.TYPE_UNSUPPORTED)
    result1 = build_result(
        "amf0-unsupported",
        AMF0,
        amf0.TYPE_UNSUPPORTED,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_recordset():
    enc1 = AMF0Encoder()
    enc1.writeType(amf0.TYPE_RECORDSET)
    result1 = build_result(
        "amf0-recordset",
        AMF0,
        amf0.TYPE_RECORDSET,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_xml_doc():
    v1 = "<a><b>hello world</b></a>"
    enc1 = AMF0Encoder()
    enc1.writeXML(xml.fromstring(v1))
    result1 = build_result(
        "amf0-xml-doc",
        AMF0,
        amf0.TYPE_XML,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_typed_object():
    v1 = HogeClass(0, "fugaaaaaaa")
    enc1 = AMF0Encoder()
    enc1.writeObject(v1)
    result1 = build_result(
        "amf0-typed-object",
        AMF0,
        amf0.TYPE_TYPEDOBJECT,
        v1.__dict__,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf0_avmplus():
    v1 = TypedObject("flex.messaging.messages.CommandMessage")
    v1['operation'] = 5
    v1['timestamp'] = 0
    enc1 = AMF0Encoder()
    enc1.writeAMF3(v1)
    result1 = build_result(
        "amf0-avmplus",
        AMF0,
        amf0.TYPE_AMF3,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

##############################
#            AMF3            #
##############################


def gen_amf3_undefined():
    enc1 = AMF3Encoder()
    enc1.writeUndefined("")
    result1 = build_result(
        "amf3-undefined",
        AMF3,
        amf3.TYPE_UNDEFINED,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_null():
    enc1 = AMF3Encoder()
    enc1.writeNull("")
    result1 = build_result(
        "amf3-null",
        AMF3,
        amf3.TYPE_NULL,
        None,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_false():
    v1 = False
    enc1 = AMF3Encoder()
    enc1.writeBoolean(v1)
    result1 = build_result(
        "amf3-boolean-false",
        AMF3,
        amf3.TYPE_BOOL_FALSE,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_true():
    v1 = True
    enc1 = AMF3Encoder()
    enc1.writeBoolean(v1)
    result1 = build_result(
        "amf3-boolean-true",
        AMF3,
        amf3.TYPE_BOOL_TRUE,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_integer():
    v1 = 128
    enc1 = AMF3Encoder()
    enc1.writeInteger(v1)
    result1 = build_result(
        "amf3-integer-128",
        AMF3,
        amf3.TYPE_INTEGER,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = 16384
    enc2 = AMF3Encoder()
    enc2.writeInteger(v2)
    result2 = build_result(
        "amf3-integer-16384",
        AMF3,
        amf3.TYPE_INTEGER,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)

    v3 = 0
    enc3 = AMF3Encoder()
    enc3.writeInteger(v3)
    result3 = build_result(
        "amf3-integer-0",
        AMF3,
        amf3.TYPE_INTEGER,
        v3,
        enc3.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result3)

    v4 = MIN_29B_INT
    enc4 = AMF3Encoder()
    enc4.writeInteger(v4)
    result4 = build_result(
        "amf3-integer-min-u29",
        AMF3,
        amf3.TYPE_INTEGER,
        v4,
        enc4.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result4)

    v5 = MAX_29B_INT
    enc5 = AMF3Encoder()
    enc5.writeInteger(v5)
    result5 = build_result(
        "amf3-integer-max-u29",
        AMF3,
        amf3.TYPE_INTEGER,
        v5,
        enc5.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result5)


def gen_amf3_double():
    v1 = 3.14
    enc1 = AMF3Encoder()
    enc1.writeNumber(v1)
    result1 = build_result(
        "amf3-double-pi",
        AMF3,
        amf3.TYPE_NUMBER,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = MIN_29B_INT - 1.0
    enc2 = AMF3Encoder()
    enc2.writeNumber(v2)
    result2 = build_result(
        "amf3-double-min-u29",
        AMF3,
        amf3.TYPE_NUMBER,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)

    v3 = MAX_29B_INT + 1.0
    enc3 = AMF3Encoder()
    enc3.writeNumber(v3)
    result3 = build_result(
        "amf3-double-max-u29",
        AMF3,
        amf3.TYPE_NUMBER,
        v3,
        enc3.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result3)


def gen_amf3_string():
    v1 = "こんにちは、世界！"
    enc1 = AMF3Encoder()
    enc1.writeString(v1)
    result1 = build_result(
        "amf3-string",
        AMF3,
        amf3.TYPE_STRING,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_xml_doc():
    v1 = "<a><b>hello world</b></a>"
    enc1 = AMF3Encoder()
    enc1.stream.write(amf3.TYPE_XML)
    enc1.serialiseString(xml.tostring(xml.fromstring(v1)))
    result1 = build_result(
        "amf3-xml-doc",
        AMF3,
        amf3.TYPE_XML,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_date():
    v1 = datetime.datetime(2005, 3, 18, 1, 58, 31)
    enc1 = AMF3Encoder()
    enc1.writeDate(v1)
    result1 = build_result(
        "amf3-date",
        AMF3,
        amf3.TYPE_DATE,
        v1.timestamp(),
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_array():
    v1 = [1.1, 2, 3.3, "こんにちは、世界！"]
    enc1 = AMF3Encoder()
    enc1.writeList(v1)
    result1 = build_result(
        "amf3-array-dense",
        AMF3,
        amf3.TYPE_ARRAY,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = MixedArray(en="Hello, world!", ja="こんにちは、世界！", zh="你好世界")
    enc2 = AMF3Encoder()
    enc2.writeDict(v2)
    result1 = build_result(
        "amf3-array-assoc",
        AMF3,
        amf3.TYPE_ARRAY,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_object():
    v1 = NoDynamicHogeClass(0, "fugaaaaaaa")
    enc1 = AMF3Encoder()
    enc1.writeObject(v1)
    result1 = build_result(
        "amf3-object",
        AMF3,
        amf3.TYPE_OBJECT,
        v1.__dict__,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = HogeClass(0, "fugaaaaaaa")
    enc2 = AMF3Encoder()
    enc2.writeObject(v2)
    enc2.writeObject(v2)
    result2 = build_result(
        "amf3-object-ref",
        AMF3,
        amf3.TYPE_OBJECT,
        v2.__dict__,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)

    v3 = NoDynamicHogeClass(0, "fugaaaaaaa")
    enc3 = AMF3Encoder()
    enc3.writeObject(v3)
    result3 = build_result(
        "amf3-object-typed",
        AMF3,
        amf3.TYPE_OBJECT,
        v3.__dict__,
        enc3.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result3)

    v4 = DynamicHogeClass(0, "fugaaaaaaa")
    enc4 = AMF3Encoder()
    enc4.writeObject(v4)
    result4 = build_result(
        "amf3-object-dynamic",
        AMF3,
        amf3.TYPE_OBJECT,
        v4.__dict__,
        enc4.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result4)

    v5 = {
        "index": 0,
        "msg": "fugaaaaaaa"
    }
    enc5 = AMF3Encoder()
    enc5.writeObject(v5)
    result5 = build_result(
        "amf3-object-hash",
        AMF3,
        amf3.TYPE_OBJECT,
        v5,
        enc5.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result5)


def gen_amf3_xml_string():
    v1 = "<a><b>hello world</b></a>"
    enc1 = AMF3Encoder()
    enc1.writeXML(xml.fromstring(v1))
    result1 = build_result(
        "amf3-xml",
        AMF3,
        amf3.TYPE_XMLSTRING,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_byte_array():
    v1 = 'hello'
    v1_b = ByteArray(v1)
    enc1 = AMF3Encoder()
    enc1.writeByteArray(v1_b)
    result1 = build_result(
        "amf3-byte-array",
        AMF3,
        amf3.TYPE_BYTEARRAY,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

    v2 = {
        "msg": "Hello, world! こんにちは、世界！",
        "index": 0,
    }
    v2_b = ByteArray()
    v2_b.writeObject(v2)
    enc2 = AMF3Encoder()
    enc2.writeByteArray(v2_b)
    result2 = build_result(
        "amf3-byte-array-object",
        AMF3,
        amf3.TYPE_BYTEARRAY,
        v2,
        enc2.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result2)


def gen_amf3_vector_int():
    v1 = IntVector([-1, 0, 1])
    enc1 = AMF3Encoder()
    enc1.writeVector(v1)
    result1 = build_result(
        "amf3-vector-int",
        AMF3,
        amf3.TYPE_INT_VECTOR,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_vector_uint():
    v1 = UintVector([0, 1, 2])
    enc1 = AMF3Encoder()
    enc1.writeVector(v1)
    result1 = build_result(
        "amf3-vector-uint",
        AMF3,
        amf3.TYPE_UINT_VECTOR,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_vector_double():
    v1 = DoubleVector([-1.1, 0.0, 1.1])
    enc1 = AMF3Encoder()
    enc1.writeVector(v1)
    result1 = build_result(
        "amf3-vector-double",
        AMF3,
        amf3.TYPE_DOUBLE_VECTOR,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_vector_object():
    v1 = ObjectVector([
        {
            "index": 0,
            "msg": "Hello, world!"
        },
        {
            "index": 1,
            "msg": "こんにちは、世界！"
        },
        {
            "index": 2,
            "msg": "你好世界"
        }
    ])
    v1.classname = "com.pyyoshi.fooclass"

    enc1 = AMF3Encoder()
    enc1.writeVector(v1)
    result1 = build_result(
        "amf3-vector-object",
        AMF3,
        amf3.TYPE_OBJECT_VECTOR,
        v1,
        enc1.stream.getvalue(),
        classname=v1.classname
    )
    write_json(OUTPUT_DIR, result1)


def gen_amf3_dictionary():
    v1 = ASDictionary(en="Hello, world!", ja="こんにちは、世界！", zh="你好世界")
    enc1 = AMF3Encoder()
    enc1.writeASDictionary(v1)
    result1 = build_result(
        "amf3-dictionary",
        AMF3,
        amf3.TYPE_DICTIONARY,
        v1,
        enc1.stream.getvalue()
    )
    write_json(OUTPUT_DIR, result1)

##############################
#            MAIN            #
##############################


def main():
    gen_amf0_number()
    gen_amf0_boolean()
    gen_amf0_string()
    gen_amf0_object()
    gen_amf0_movieclip()
    gen_amf0_null()
    gen_amf0_undefined()
    gen_amf0_reference()
    gen_amf0_ecma_array()
    gen_amf0_object_end()
    gen_amf0_strict_array()
    gen_amf0_date()
    gen_amf0_longstring()
    gen_amf0_unsupported()
    gen_amf0_recordset()
    gen_amf0_xml_doc()
    gen_amf0_typed_object()
    gen_amf0_avmplus()

    gen_amf3_undefined()
    gen_amf3_null()
    gen_amf3_false()
    gen_amf3_true()
    gen_amf3_integer()
    gen_amf3_double()
    gen_amf3_string()
    gen_amf3_xml_doc()
    gen_amf3_date()
    gen_amf3_array()
    gen_amf3_object()
    gen_amf3_xml_string()
    gen_amf3_byte_array()
    gen_amf3_vector_int()
    gen_amf3_vector_uint()
    gen_amf3_vector_double()
    gen_amf3_vector_object()
    gen_amf3_dictionary()


if __name__ == '__main__':
    main()
